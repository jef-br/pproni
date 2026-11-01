//! Application shell: folder discovery, localized prompts, batch orchestration, completion alert.

pub mod batch;
pub mod i18n;
pub mod progress;
pub mod theme;

#[cfg(windows)]
mod console;
#[cfg(windows)]
mod header;
#[cfg(windows)]
mod desktop;
#[cfg(windows)]
mod shell;

/// Entry point when launched with no arguments (the double-click path).
#[cfg(windows)]
pub fn run_default() {
    shell::run();
}

/// Non-Windows: the folder flow is Windows-only.
#[cfg(not(windows))]
pub fn run_default() {
    eprintln!("usage: {} <input_dir> <output_dir>", env!("CARGO_BIN_NAME"));
}
