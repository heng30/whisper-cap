// cargo run --example main_srt

use anyhow::{bail, Result};
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};
use transcribe::{subtitle, wav, whisper, whisper_lang::WhisperLang, SegmentCallbackData};

#[tokio::main]
async fn main() -> Result<()> {
    let model_path = PathBuf::from("./examples/models/ggml-base.bin");
    // let audio_path = PathBuf::from("./examples/data/test-20s.mp3");
    let audio_path = PathBuf::from("./examples/data/test.mp4");
    // let audio_path = PathBuf::from("/home/blue/Videos/obs/2025-08-28_14-49-05.mkv");
    let output_audio_path = PathBuf::from("./examples/tmp/test-20s.wav");

    if !model_path.exists() {
        bail!("Can't find modle: {}", model_path.display());
    }

    if !audio_path.exists() {
        bail!("Can't find modle: {}", audio_path.display());
    }

    if wav::is_whisper_compatible(&audio_path).is_err() {
        println!("[Info] Convert to whisper compatible audio file...");

        whisper::convert_to_compatible_audio(
            &audio_path,
            &output_audio_path,
            Arc::new(AtomicBool::new(false)),
            |progress| println!("convert to auido progress: {progress}%"),
        )?;
    }

    let config = whisper::WhisperConfig::new(model_path)
        .with_language(WhisperLang::Chinese.to_string())
        .with_temperature(0.0);

    let mut index = 0;
    match whisper::transcribe_file(
        config,
        output_audio_path,
        |v: i32| println!("whisper progress: {v}"),
        move |segment: SegmentCallbackData| {
            index += 1;
            let contents = subtitle::subtitle_to_srt(&segment.into());
            let contents = subtitle::convert_traditional_to_simplified_chinese(&contents);
            println!("---------- {index}--------------");
            println!("{contents}\n");
        },
        || false,
    )
    .await
    {
        Ok(transcription) => {
            let mut items = subtitle::transcription_to_subtitle(&transcription);
            for item in items.iter_mut() {
                item.text = subtitle::convert_traditional_to_simplified_chinese(&item.text);
                let contents = subtitle::subtitle_to_srt(item);
                println!("{}\n", contents);
            }

            subtitle::save_as_srt(&items, "./examples/tmp/output.srt")?;
        }
        Err(e) => {
            bail!("transcribe_file error: {e}");
        }
    }

    Ok(())
}
