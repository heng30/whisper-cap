// cargo run --example main_model_downloader

use anyhow::Result;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use transcribe::whisper_model_downloader::{ModelDownloader, MODEL_NAMES};

#[tokio::main]
async fn main() -> Result<()> {
    for name in MODEL_NAMES {
        let downloader = ModelDownloader::new(name.to_string(), "./examples/tmp".to_string());

        let cancel_sig = Arc::new(AtomicBool::new(false));
        let cancel_sig_duplicate = cancel_sig.clone();

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            cancel_sig_duplicate.store(true, Ordering::Relaxed);
        });

        let model_name = name.to_string();
        downloader
            .download_model(cancel_sig, move |downloaded, total_size, progress| {
                println!("{model_name}: {downloaded}/{total_size} => {progress:.2}%")
            })
            .await?;
    }
    Ok(())
}
