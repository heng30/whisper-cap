use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::{
    fs,
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub const WHISPER_MODELS_WEBSITE: &str = "https://huggingface.co/ggerganov/whisper.cpp";
pub const OFFICIAL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub const MODEL_NAMES: [&str; 7] = [
    "ggml-tiny.bin",
    "ggml-base.bin",
    "ggml-small.bin",
    "ggml-medium.bin",
    "ggml-large-v1.bin",
    "ggml-large-v2.bin",
    "ggml-large-v3.bin",
];

pub enum DownloadStatus {
    Finsished,
    Cancelled,
    Partial,
}

#[derive(Debug, Clone)]
pub struct ModelDownloader {
    pub base_url: String,
    pub model_name: String,
    pub save_dir: String,
}

impl ModelDownloader {
    pub fn new(model_name: String, save_dir: String) -> ModelDownloader {
        ModelDownloader {
            base_url: OFFICIAL_BASE_URL.to_string(),
            model_name,
            save_dir,
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn model_path(&self) -> String {
        format!("{}/{}", self.save_dir, self.model_name)
    }

    pub async fn download_model(
        &self,
        is_cancel: Arc<AtomicBool>,
        mut progress_cb: impl FnMut(u64, u64, f32) + 'static,
    ) -> Result<DownloadStatus> {
        let filepath = self.model_path();
        let filepath_tmp = format!("{}/{}.tmp", self.save_dir, self.model_name);
        let url = format!("{}/{}", self.base_url, self.model_name);

        let mut save_file = fs::File::create(&filepath_tmp)
            .with_context(|| format!("create {} failed", filepath_tmp))?;

        let response = Client::new()
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Failed to send request to {}", url))?;

        let total_size = response
            .content_length()
            .ok_or_else(|| anyhow::anyhow!("Failed to get content length from response"))?;

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            if is_cancel.load(Ordering::Relaxed) {
                return Ok(DownloadStatus::Cancelled);
            }

            let chunk = chunk.with_context(|| "Failed to read chunk from response")?;
            save_file
                .write_all(&chunk)
                .with_context(|| "Failed to write chunk to file")?;

            downloaded += chunk.len() as u64;

            let progress = (downloaded as f64 / total_size as f64 * 100.0) as f32;
            progress_cb(downloaded, total_size, progress);
        }

        if total_size == downloaded {
            _ = fs::rename(&filepath_tmp, &filepath);
            Ok(DownloadStatus::Finsished)
        } else {
            Ok(DownloadStatus::Partial)
        }
    }
}
