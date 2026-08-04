//! Quick verification of the local Hy-MT translation path without the app:
//! loads the GGUF model and translates a Japanese sentence.
use live_translator_desktop_lib::local_mt::LocalMtTranslator;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let models_dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| {
        "/Users/Mon3tr/mysoftware/同传/live-translator/models".to_owned()
    }));
    let text = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "こんにちは、今日はいい天気ですね。".to_owned());

    println!("loading Hy-MT model from {:?} ...", models_dir);
    let translator = LocalMtTranslator::new(&models_dir)?;
    println!("model loaded");

    let started = std::time::Instant::now();
    let translated = translator.translate(&text)?;
    println!("translate took {:?}", started.elapsed());
    println!("JA: {text}");
    println!("ZH: {translated}");
    Ok(())
}
