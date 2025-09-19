use super::ProgressStatus;
use crate::wav;
use anyhow::{bail, Result};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub struct EnergyVAD {
    pub threshold: f32,
    pub sample_rate: u32,
    pub frame_size_ms: u64,
    pub frame_shift_ms: u64,
}

impl EnergyVAD {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            threshold: 0.1,
            sample_rate,
            frame_size_ms: 200,
            frame_shift_ms: 100,
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    pub fn with_frame_size_ms(mut self, ms: u64) -> Self {
        self.frame_size_ms = ms;
        self
    }

    pub fn with_frame_shift_ms(mut self, ms: u64) -> Self {
        self.frame_shift_ms = ms;
        self
    }

    pub fn calculate_rms(&self, samples: &[f32]) -> f32 {
        let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
        (sum_squares / samples.len() as f32).sqrt()
    }

    pub fn contain_speech(&self, samples: &[f32]) -> bool {
        if samples.is_empty() {
            return false;
        }

        let rms = self.calculate_rms(samples);
        // println!("--------- {rms}, {}", self.threshold);
        rms > self.threshold
    }

    pub fn detect_all_active_segments(&self, samples: &[f32]) -> Vec<(u64, u64)> {
        let frame_size = ((self.sample_rate as u64 * self.frame_size_ms) as f32 / 1000.0) as usize;
        let frame_shift =
            ((self.sample_rate as u64 * self.frame_shift_ms) as f32 / 1000.0) as usize;

        let mut segments = Vec::new();
        let (mut start_ms, mut end_ms) = (0, 0);
        let mut in_active_segment = false;
        let total_ms = ((samples.len() as f64 / self.sample_rate as f64) * 1000.0) as u64;

        for (index, offset) in (0..samples.len()).step_by(frame_shift).enumerate() {
            let frame_end = std::cmp::min(offset + frame_size, samples.len());
            if offset >= frame_end {
                break;
            }

            let frame = &samples[offset..frame_end];
            let is_speech = self.contain_speech(frame);

            if is_speech {
                in_active_segment = true;
                end_ms += self.frame_shift_ms;
            } else {
                if in_active_segment {
                    in_active_segment = false;
                    segments.push((start_ms, end_ms));
                }
                start_ms = index as u64 * self.frame_shift_ms;
                end_ms = start_ms;
            }
        }

        if let Some((_, last_end_ms)) = segments.last() {
            if start_ms >= *last_end_ms && *last_end_ms < total_ms {
                segments.push((start_ms, total_ms));
            }
        }

        segments
    }

    fn detect_silent_offset_ms(&self, samples: &[f32]) -> u64 {
        let frame_size = ((self.sample_rate as u64 * self.frame_size_ms) as f32 / 1000.0) as usize;
        let frame_shift =
            ((self.sample_rate as u64 * self.frame_shift_ms) as f32 / 1000.0) as usize;

        for (index, offset) in (0..samples.len()).step_by(frame_shift).enumerate() {
            let frame_end = std::cmp::min(offset + frame_size, samples.len());
            if offset >= frame_end {
                return 0;
            }

            let frame = &samples[offset..frame_end];
            let is_speech = self.contain_speech(frame);

            if is_speech {
                if index == 0 {
                    return 0;
                } else {
                    return index as u64 * self.frame_shift_ms;
                }
            }
        }

        return 0;
    }
}

pub fn trim_start_slient_duration_of_audio(
    audio_path: impl AsRef<Path>,
    timestamps: &[(u64, u64)], // (ms, ms)
    threshold: f32,            // [0, 1]
    cancel: Arc<AtomicBool>,
    mut progress_cb: impl FnMut(i32) + 'static,
) -> Result<(Vec<(u64, u64)>, ProgressStatus)> {
    let audio_data = wav::read_file(&audio_path)?;

    let audio_samples = if !audio_data.is_whisper_compatible() {
        if audio_data.config.sample_rate != 16000 {
            bail!(
                "Not compatible with whisper. Actual sample rate {}, expect 16kHz",
                audio_data.config.sample_rate
            );
        }

        if audio_data.config.channels > 1 {
            audio_data.to_mono().samples
        } else {
            audio_data.samples.clone()
        }
    } else {
        audio_data.samples.clone()
    };

    let mut output_timestamps = vec![];
    let sample_rate = audio_data.config.sample_rate;
    let total_indexs = audio_samples.len();

    for (index, (start_ms, end_ms)) in timestamps.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Ok((vec![], ProgressStatus::Cancelled));
        }

        let vad = EnergyVAD::new(sample_rate).with_threshold(threshold);
        let start_index = ((sample_rate as u64 * start_ms) as f64 / 1000.0) as usize;
        let end_index =
            (((sample_rate as u64 * end_ms) as f64 / 1000.0) as usize).min(total_indexs);

        if start_index >= end_index {
            output_timestamps.push((*start_ms, *end_ms));
            continue;
        }

        let segmemt = &audio_samples[start_index..end_index];
        let silent_offset = vad.detect_silent_offset_ms(segmemt);

        // println!("------- {index}: {silent_offset}");

        if silent_offset == 0 {
            output_timestamps.push((*start_ms, *end_ms));
        } else {
            let offset_ms = if silent_offset > vad.frame_size_ms {
                start_ms + silent_offset - vad.frame_size_ms
            } else {
                *start_ms
            };

            if offset_ms >= *end_ms {
                output_timestamps.push((*start_ms, *end_ms));
            } else {
                output_timestamps.push((offset_ms, *end_ms));
            }
        }

        let progress = (index + 1) * 100 / timestamps.len();
        progress_cb(progress as i32);
    }

    Ok((output_timestamps, ProgressStatus::Finished))
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test test_vad_detect -- --no-capture
    #[test]
    fn test_vad_detect() -> Result<()> {
        let audio_path = "./examples/data/test-20.wav";
        let audio_data = wav::read_file(audio_path)?;

        let vad = EnergyVAD::new(audio_data.config.sample_rate);
        let timestamps = vad.detect_all_active_segments(&audio_data.samples);

        for (index, (start_ms, end_ms)) in timestamps.into_iter().enumerate() {
            println!(
                "{}: {} -> {}",
                index + 1,
                start_ms as f64 / 1000.0,
                end_ms as f64 / 1000.0
            );
        }

        Ok(())
    }

    // cargo test test_trim_start_slient_duration_of_audio -- --no-capture
    #[test]
    fn test_trim_start_slient_duration_of_audio() -> Result<()> {
        let audio_path = "./examples/data/test-20.wav";
        let timestamps = vec![
            (0, 3_000),
            (3_000, 5_000),
            (5_000, 7_000),
            (7_000, 10_000),
            (10_000, 14_500),
            (14_500, 20_000),
        ];

        let (output_timestamps, status) = trim_start_slient_duration_of_audio(
            audio_path,
            &timestamps,
            0.01,
            Arc::new(AtomicBool::new(false)),
            move |v| println!("progress: {v}%"),
        )?;

        println!("status: {status:?}");

        assert_eq!(timestamps.len(), output_timestamps.len());

        for (index, (start_ms, end_ms)) in output_timestamps.into_iter().enumerate() {
            println!(
                "{}: ({} -> {}) => ({} -> {})",
                index + 1,
                timestamps[index].0 as f64 / 1000.0,
                timestamps[index].1 as f64 / 1000.0,
                start_ms as f64 / 1000.0,
                end_ms as f64 / 1000.0
            );
        }

        Ok(())
    }
}
