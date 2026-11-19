//! Batch product-image repositioner.
//!
//! No args: the desktop flow. `<input_dir> <output_dir> [backup_dir]`: a plain batch;
//! `--enable-headcuts` anywhere turns headcutting on. See docs/architecture/modules/main.md.

mod app;

use pproni::core::options::Options;
use std::path::PathBuf;

fn main() {
    // A missing ONNX Runtime or model is an environment fault, not an image outcome: stop before
    // touching a single file rather than processing the whole batch with a dead classifier.
    if let Err(e) = pproni::core::preflight() {
        eprintln!("cannot start: {e}");
        std::process::exit(1);
    }

    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // This path is piped and cannot ask, so the setting the desktop flow asks for is a flag here.
    let options = Options { enable_headcuts: args.iter().any(|a| a == "--enable-headcuts") };
    args.retain(|a| a != "--enable-headcuts");
    if args.len() == 2 || args.len() == 3 {
        let input = PathBuf::from(&args[0]);
        let output = PathBuf::from(&args[1]);
        let backup = match args.get(2) {
            Some(path) => PathBuf::from(path),
            None => output
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("PPRONI-BACKUP"),
        };
        // Dev/test path only: the desktop screens clear the console, so the backend is not shown there.
        eprintln!("classifier runs on: {}", pproni::core::backend());
        // English: this path is piped, so the bar — and the only localized string it would
        // reach for — is disabled anyway.
        let summary = app::batch::run(&input, &output, &backup, &[], app::i18n::Lang::En, &options);
        println!(
            "{} ok, {} failed, {:.2}s",
            summary.ok, summary.failed, summary.seconds
        );
        return;
    }
    app::run_default();
}
