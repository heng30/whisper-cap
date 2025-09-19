use anyhow::{anyhow, bail, Result};
use hound::{SampleFormat, WavReader};
use log::debug;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub bit_depth: u16,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            bit_depth: 16,
        }
    }
}

impl AudioConfig {
    pub fn new(sample_rate: u32, channels: u16, bit_depth: u16) -> Self {
        Self {
            sample_rate,
            channels,
            bit_depth,
        }
    }

    pub fn whisper_optimized() -> Self {
        Self::default()
    }

    pub fn is_whisper_compatible(&self) -> bool {
        self.sample_rate == 16000 && self.channels == 1 && self.bit_depth == 16
    }
}

#[derive(Debug, Clone)]
pub struct AudioData {
    pub samples: Vec<f32>, // range: (-1.0 , 1.0）
    pub config: AudioConfig,
}

impl AudioData {
    pub fn new(samples: Vec<f32>, config: AudioConfig) -> Self {
        Self { samples, config }
    }

    pub fn duration(&self) -> f64 {
        let frames = self.samples.len() / self.config.channels as usize;
        frames as f64 / self.config.sample_rate as f64
    }

    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.config.channels as usize
    }

    pub fn to_mono(&self) -> AudioData {
        if self.config.channels == 1 {
            return self.clone();
        }

        let frame_count = self.frame_count();
        let mut mono_samples = Vec::with_capacity(frame_count);

        for frame in 0..frame_count {
            let mut sum = 0.0;
            for channel in 0..self.config.channels {
                let index = frame * self.config.channels as usize + channel as usize;
                if index < self.samples.len() {
                    sum += self.samples[index];
                }
            }
            mono_samples.push(sum / self.config.channels as f32);
        }

        let mono_config = AudioConfig {
            channels: 1,
            ..self.config
        };

        AudioData::new(mono_samples, mono_config)
    }

    pub fn normalize(&mut self) {
        if self.samples.is_empty() {
            return;
        }

        let max_abs = self.samples.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
        if max_abs > 0.0 && max_abs != 1.0 {
            let scale = 1.0 / max_abs;
            for sample in &mut self.samples {
                *sample *= scale;
            }

            debug!("Normalized audio. Scale factor: {scale:.3}");
        }
    }

    pub fn apply_gain(&mut self, gain_db: f32) {
        let gain_linear = 10.0f32.powf(gain_db / 20.0);
        for sample in &mut self.samples {
            *sample *= gain_linear;
            *sample = sample.clamp(-1.0, 1.0); // prevent clipping
        }

        debug!("Apply gain: {gain_db:.1} dB (gain linear: {gain_linear:.3})");
    }

    pub fn is_whisper_compatible(&self) -> bool {
        self.config.is_whisper_compatible()
    }
}

pub fn read_file<P: AsRef<Path>>(path: P) -> Result<AudioData> {
    let path = path.as_ref();
    if !path.exists() {
        bail!("file not found {}", path.display());
    }

    let mut reader = WavReader::open(path).map_err(|e| anyhow!("open wav file failed: {e}"))?;

    let spec = reader.spec();
    let config = AudioConfig {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        bit_depth: spec.bits_per_sample,
    };

    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Read float point sample failed: {e}"))?,

        SampleFormat::Int => {
            let int_samples: Result<Vec<i32>, _> = reader.samples::<i32>().collect();
            match int_samples {
                Ok(samples) => {
                    // Convert floating point according to bit depth
                    let max_value = match spec.bits_per_sample {
                        16 => i16::MAX as f32,
                        24 => 8388607.0, // 2^23 - 1
                        32 => i32::MAX as f32,
                        _ => bail!("Unsupported bits per sample: {}", spec.bits_per_sample),
                    };
                    samples.into_iter().map(|x| x as f32 / max_value).collect()
                }
                Err(e) => bail!("Read file sample failed: {e}"),
            }
        }
    };

    Ok(AudioData::new(samples, config))
}

pub fn is_whisper_compatible(path: impl AsRef<Path>) -> Result<()> {
    let reader = WavReader::open(path.as_ref())
        .map_err(|e| anyhow!("Failed to open {}. Error: {e}", path.as_ref().display()))?;
    let spec = reader.spec();

    if spec.sample_rate != 16000 {
        bail!(
            "Sample rate mismatch. Expected: 16000, actual: {}",
            spec.sample_rate
        );
    }

    if spec.channels != 1 {
        bail!("Channel mismatch. Expected: 1, actual: {}", spec.channels);
    }

    if spec.bits_per_sample != 16 {
        bail!(
            "Format not supported. Expected 16 bit PCM, actual {} bit PCM",
            spec.bits_per_sample
        );
    }

    Ok(())
}
