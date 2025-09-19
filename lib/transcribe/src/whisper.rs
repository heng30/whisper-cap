use super::wav::{self, AudioData};
use anyhow::{anyhow, bail, Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
use whisper_rs::{
    FullParams, SamplingStrategy, SegmentCallbackData, WhisperContext, WhisperContextParameters,
    WhisperState, WhisperVadParams,
};

const GGML_SILERO_VAD_MODEL: &'static [u8] = include_bytes!("../data/ggml-silero-v5.1.2.bin");

#[derive(Clone, Debug)]
pub struct WhisperConfig {
    pub model_path: PathBuf,
    pub vad_model_path: Option<PathBuf>,
    pub language: Option<String>, // "zh", "en"，None is auto detect
    pub translate: bool,
    pub n_threads: i32,
    pub temperature: f32,
    pub max_segment_length: Option<u32>,
    pub initial_prompt: Option<String>,
    pub debug_mode: bool,
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/ggml-base.bin"),
            vad_model_path: None,
            language: None,
            translate: false,
            n_threads: num_cpus::get().min(8) as i32,
            temperature: 0.0,
            max_segment_length: None,
            initial_prompt: None,
            debug_mode: false,
        }
    }
}

impl WhisperConfig {
    pub fn new<P: Into<PathBuf>>(model_path: P) -> Self {
        Self {
            model_path: model_path.into(),
            ..Default::default()
        }
    }

    pub fn with_vad_model_path<S: Into<PathBuf>>(mut self, path: S) -> Self {
        self.vad_model_path = Some(path.into());
        self
    }

    pub fn with_language<S: Into<String>>(mut self, language: S) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn with_translate(mut self, translate: bool) -> Self {
        self.translate = translate;
        self
    }

    pub fn with_threads(mut self, n_threads: i32) -> Self {
        self.n_threads = n_threads;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.clamp(0.0, 1.0);
        self
    }

    pub fn with_initial_prompt<S: Into<String>>(mut self, prompt: S) -> Self {
        self.initial_prompt = Some(prompt.into());
        self
    }

    pub fn with_debug_mode(mut self, debug_mode: bool) -> Self {
        self.debug_mode = debug_mode;
        self
    }

    pub fn validate(&self) -> Result<()> {
        if !self.model_path.exists() {
            bail!("model path not exist: {}", self.model_path.display());
        }

        if self.n_threads <= 0 {
            bail!("n_threads is 0");
        }

        if !(0.0..=1.0).contains(&self.temperature) {
            bail!("temperature should between 0.0 and 1.0");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub index: i32,
    pub start_time: u64, // ms
    pub end_time: u64,   // ms
    pub text: String,
    pub confidence: f32, // (0.0-1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<TranscriptionSegment>,
    pub processing_time: u64, // ms
    pub audio_duration: u64,  // ms
}

impl TranscriptionResult {
    pub fn real_time_factor(&self) -> f64 {
        if self.audio_duration == 0 {
            return 0.0;
        }
        self.processing_time as f64 / self.audio_duration as f64
    }

    pub fn average_confidence(&self) -> f32 {
        if self.segments.is_empty() {
            return 0.0;
        }

        let total: f32 = self.segments.iter().map(|s| s.confidence).sum();
        total / self.segments.len() as f32
    }

    pub fn filter_by_confidence(&self, min_confidence: f32) -> TranscriptionResult {
        let filtered_segments: Vec<_> = self
            .segments
            .iter()
            .filter(|s| s.confidence >= min_confidence)
            .cloned()
            .collect();

        let filtered_text = filtered_segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        TranscriptionResult {
            text: filtered_text,
            language: self.language.clone(),
            segments: filtered_segments,
            processing_time: self.processing_time,
            audio_duration: self.audio_duration,
        }
    }
}

pub struct WhisperTranscriber {
    context: Arc<WhisperContext>,
    config: WhisperConfig,
}

impl WhisperTranscriber {
    pub fn new(config: WhisperConfig) -> Result<Self> {
        config.validate()?;

        debug!("Load Whisper model: {}", config.model_path.display());

        let ctx_params = WhisperContextParameters::default();
        let context = WhisperContext::new_with_params(
            config.model_path.to_string_lossy().as_ref(),
            ctx_params,
        )
        .map_err(|e| anyhow!("Load Whisper model error: {e}"))?;

        Ok(Self {
            context: Arc::new(context),
            config,
        })
    }

    pub async fn transcribe_file<P: AsRef<Path>>(
        &self,
        audio_path: P,
        progress_cb: impl FnMut(i32) + 'static,
        segmemnt_cb: impl FnMut(SegmentCallbackData) + 'static,
        abort_cb: impl FnMut() -> bool + 'static,
    ) -> Result<TranscriptionResult> {
        is_valid_aduio_file(&audio_path)?;
        debug!("Start transcribe: {}", audio_path.as_ref().display());

        let audio_data = wav::read_file(&audio_path)?;
        self.transcribe_audio_data(&audio_data, progress_cb, segmemnt_cb, abort_cb)
            .await
    }

    pub async fn transcribe_audio_data(
        &self,
        audio_data: &AudioData,
        progress_cb: impl FnMut(i32) + 'static,
        segmemnt_cb: impl FnMut(SegmentCallbackData) + 'static,
        abort_cb: impl FnMut() -> bool + 'static,
    ) -> Result<TranscriptionResult> {
        let start_time = std::time::Instant::now();

        let audio_samples = if !audio_data.is_whisper_compatible() {
            self.prepare_audio_samples(audio_data)?
        } else {
            audio_data.samples.clone()
        };

        debug!(
            "Start whisper infer，audio duration: {:.2}s",
            audio_data.duration()
        );

        let mut state = self
            .context
            .create_state()
            .map_err(|e| anyhow!("Create whisper state failed: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.config.n_threads);
        params.set_translate(self.config.translate);
        params.set_debug_mode(self.config.debug_mode);
        params.set_temperature(self.config.temperature);
        params.set_language(self.config.language.as_ref().map(|x| x.as_str()));
        params.set_token_timestamps(true);

        params.set_progress_callback_safe(progress_cb);
        params.set_segment_callback_safe(segmemnt_cb);
        params.set_abort_callback_safe(abort_cb);

        if let Some(path) = &self.config.vad_model_path {
            if !path.exists() {
                bail!("No found vad model path: {}", path.display());
            }

            params.set_vad_model_path(Some(&path.to_string_lossy().to_string()));
            params.set_vad_params(WhisperVadParams::default());
            params.enable_vad(true);
        }

        if let Some(prompt) = &self.config.initial_prompt {
            params.set_initial_prompt(prompt.as_str());
        }

        state
            .full(params, &audio_samples)
            .map_err(|e| anyhow!("Whisper transcribe failed: {e}"))?;

        let result =
            self.extract_transcription_result(&state, audio_data.duration(), start_time)?;

        debug!(
            "Transcript finished，real time factor: {:.2}x",
            result.real_time_factor()
        );

        Ok(result)
    }

    fn prepare_audio_samples(&self, audio_data: &AudioData) -> Result<Vec<f32>> {
        let mut samples = audio_data.samples.clone();

        if audio_data.config.channels > 1 {
            let mono_data = audio_data.to_mono();
            samples = mono_data.samples;
            debug!("Finished converting to mono channel");
        }

        if audio_data.config.sample_rate != 16000 {
            bail!(
                "Not compatible with whisper. Actual sample rate {}, expect 16kHz",
                audio_data.config.sample_rate
            );
        }

        Ok(samples)
    }

    fn extract_transcription_result(
        &self,
        state: &WhisperState,
        audio_duration: f64,
        start_time: std::time::Instant,
    ) -> Result<TranscriptionResult> {
        let audio_duration_ms = (audio_duration * 1000.0) as u64;

        let num_segments = state.full_n_segments();

        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            let Some(segment) = state.get_segment(i) else {
                continue;
            };

            let segment_text = segment.to_str().unwrap_or("").trim().to_string();

            if segment_text.is_empty() {
                continue;
            }

            let start_time = (segment.start_timestamp() as u64) * 10;
            let end_time = (segment.end_timestamp() as u64) * 10;
            let confidence = self.calculate_segment_confidence(state, i)?;

            segments.push(TranscriptionSegment {
                index: i as i32 + 1,
                start_time,
                end_time,
                text: segment_text.clone(),
                confidence,
            });

            if !full_text.is_empty() {
                full_text.push(' ');
            }
            full_text.push_str(&segment_text);
        }

        let processing_time = start_time.elapsed().as_millis() as u64;
        Ok(TranscriptionResult {
            text: full_text,
            language: self.config.language.clone(),
            segments,
            processing_time,
            audio_duration: audio_duration_ms,
        })
    }

    fn calculate_segment_confidence(
        &self,
        state: &WhisperState,
        segment_index: i32,
    ) -> Result<f32> {
        let Some(segment) = state.get_segment(segment_index) else {
            return Ok(0.0);
        };
        let token_count = segment.n_tokens();

        if token_count == 0 {
            return Ok(0.0);
        }

        let mut total_prob = 0.0;
        let mut valid_tokens = 0;

        for token_index in 0..token_count {
            if let Some(token) = segment.get_token(token_index) {
                total_prob += token.token_probability();
                valid_tokens += 1;
            }
        }

        if valid_tokens > 0 {
            Ok(total_prob / valid_tokens as f32)
        } else {
            Ok(0.5)
        }
    }
}

pub fn convert_to_compatible_audio(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    cancel: Arc<AtomicBool>,
    progress_cb: impl FnMut(i32) + 'static,
) -> Result<()> {
    is_valid_aduio_file(&output)?;
    ffmpeg::convert_to_whisper_compatible_audio(&input, &output, cancel, progress_cb)?;
    wav::is_whisper_compatible(&output)?;

    Ok(())
}

pub async fn transcribe_file(
    config: WhisperConfig,
    audio_path: impl AsRef<Path>,
    progress_cb: impl FnMut(i32) + 'static,
    segmemnt_cb: impl FnMut(SegmentCallbackData) + 'static,
    abort_cb: impl FnMut() -> bool + 'static,
) -> Result<TranscriptionResult> {
    let transcriber = WhisperTranscriber::new(config)?;
    transcriber
        .transcribe_file(audio_path, progress_cb, segmemnt_cb, abort_cb)
        .await
}

pub fn save_ggml_silero_vad_model(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    fs::write(&path, GGML_SILERO_VAD_MODEL)
        .with_context(|| format!("save {} failed", path.display()))?;

    Ok(())
}

fn is_valid_aduio_file(audio_path: impl AsRef<Path>) -> Result<()> {
    if !audio_path
        .as_ref()
        .to_str()
        .unwrap_or_default()
        .to_lowercase()
        .ends_with(".wav")
    {
        bail!("Only support wav format file");
    }

    Ok(())
}
