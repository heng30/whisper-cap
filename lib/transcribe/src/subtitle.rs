use super::whisper::{TranscriptionResult, TranscriptionSegment};
use anyhow::{Context, Result};
use chrono::{NaiveTime, Timelike};
use std::{fs, path::Path};
use whisper_rs::SegmentCallbackData;

#[derive(Debug, Clone, Default)]
pub struct Subtitle {
    pub index: i32,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub text: String,
}

impl From<SegmentCallbackData> for Subtitle {
    fn from(segment: SegmentCallbackData) -> Self {
        Subtitle {
            index: segment.segment + 1,
            start_timestamp: (segment.start_timestamp as u64) * 10,
            end_timestamp: (segment.end_timestamp as u64) * 10,
            text: segment.text,
        }
    }
}

impl From<&TranscriptionSegment> for Subtitle {
    fn from(segment: &TranscriptionSegment) -> Self {
        Subtitle {
            index: segment.index,
            start_timestamp: segment.start_time,
            end_timestamp: segment.end_time,
            text: segment.text.clone(),
        }
    }
}

pub fn transcription_to_subtitle(transcription: &TranscriptionResult) -> Vec<Subtitle> {
    let mut item = vec![];

    for segment in transcription.segments.iter() {
        item.push(segment.into());
    }

    item
}

pub fn ms_to_srt_timestamp(milliseconds: u64) -> String {
    ms_to_timestamp(milliseconds, ",")
}

pub fn ms_to_vtt_timestamp(milliseconds: u64) -> String {
    ms_to_timestamp(milliseconds, ".")
}

fn ms_to_timestamp(milliseconds: u64, ms_sep: &str) -> String {
    let total_seconds = milliseconds / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    let millis = milliseconds % 1000;

    format!(
        "{:02}:{:02}:{:02}{ms_sep}{:03}",
        hours, minutes, seconds, millis
    )
}

pub fn srt_timestamp_to_ms(timestamp: &str) -> Result<u64> {
    let time = NaiveTime::parse_from_str(timestamp, "%H:%M:%S,%f")
        .with_context(|| format!("Invalid srt timestamp {timestamp}"))?;

    Ok((time.hour() as u64 * 3600000)
        + (time.minute() as u64 * 60000)
        + (time.second() as u64 * 1000)
        // This's not a bug，chrono would parse ',%f' into nanosecond field
        + (time.nanosecond() as u64))
}

pub fn valid_srt_timestamp(timestamp: &str) -> bool {
    srt_timestamp_to_ms(timestamp).is_ok()
}

pub fn subtitle_to_srt(subtitle: &Subtitle) -> String {
    format!(
        "{}\n{} --> {}\n{}",
        subtitle.index,
        ms_to_srt_timestamp(subtitle.start_timestamp),
        ms_to_srt_timestamp(subtitle.end_timestamp),
        subtitle.text
    )
}

pub fn subtitle_to_vtt(subtitle: &Subtitle) -> String {
    format!(
        "{}\n{} --> {}\n{}",
        subtitle.index,
        ms_to_vtt_timestamp(subtitle.start_timestamp),
        ms_to_vtt_timestamp(subtitle.end_timestamp),
        subtitle.text
    )
}

pub fn subtitle_to_plain(subtitle: &Subtitle) -> String {
    format!("{}", subtitle.text)
}

pub fn save_as_srt(subtitle: &[Subtitle], path: impl AsRef<Path>) -> Result<()> {
    let contents = subtitle
        .iter()
        .map(|item| format!("{}\n\n", subtitle_to_srt(&item)))
        .collect::<String>();

    fs::write(path.as_ref(), contents)
        .with_context(|| format!("Save {} failed", path.as_ref().display()))?;

    Ok(())
}

pub fn save_as_vtt(subtitle: &[Subtitle], path: impl AsRef<Path>) -> Result<()> {
    let contents = subtitle
        .iter()
        .map(|item| format!("{}\n\n", subtitle_to_vtt(&item)))
        .collect::<String>();

    fs::write(path.as_ref(), contents)
        .with_context(|| format!("Save {} failed", path.as_ref().display()))?;

    Ok(())
}

pub fn save_as_txt(subtitle: &[Subtitle], path: impl AsRef<Path>) -> Result<()> {
    let contents = subtitle
        .iter()
        .map(|item| format!("{}\n\n", subtitle_to_plain(&item)))
        .collect::<String>();

    fs::write(path.as_ref(), contents)
        .with_context(|| format!("Save {} failed", path.as_ref().display()))?;

    Ok(())
}

pub fn convert_traditional_to_simplified_chinese(text: &str) -> String {
    fast2s::convert(text)
}
