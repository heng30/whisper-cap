pub mod subtitle;
pub mod vad;
pub mod wav;
pub mod whisper;
pub mod whisper_lang;
pub mod whisper_model_downloader;

pub use whisper_rs::SegmentCallbackData;

#[derive(Debug, Clone)]
pub enum ProgressStatus {
    Finished,
    Cancelled,
}
