//! Batch processing: enumerate the images in `input` and its direct subfolders, and process each.
//! Matches the folder-bootstrap flow in `docs/diagrams/JBA2B.drawio.svg`.

use super::i18n::Lang;
use super::progress::Progress;
use pproni::core;
use pproni::core::options::Options;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub struct Summary {
    pub ok: usize,
    pub failed: usize,
    pub seconds: f64,
    /// One line per failed file, collected not printed: the progress bar owns the screen until the
    /// shell reports them.
    pub failures: Vec<String>,
}

const SUPPORTED: &[&str] = &["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"];

/// Process every supported image in `input` into `output`, in parallel. Originals of successes move
/// to `backup` (core's exporter does it); failures stay in `input` untouched. `footer`: the lines
/// held under the progress bar for the whole run. `options` goes unchanged to every file.
pub fn run(
    input: &Path,
    output: &Path,
    backup: &Path,
    footer: &[String],
    lang: Lang,
    options: &Options,
) -> Summary {
    let start = Instant::now();
    let _ = std::fs::create_dir_all(output);
    let _ = std::fs::create_dir_all(backup);

    let ok = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let jobs = collect(input, output, backup);
    let progress = Progress::new(jobs.len(), footer, lang);
    jobs.par_iter().for_each(|(src, dest, backup_dest)| {
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(parent) = backup_dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match core::process_file(src, dest, backup_dest, options) {
            Ok(()) => {
                ok.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                if let Ok(mut list) = failures.lock() {
                    list.push(format!("failed: {} — {e}", src.display()));
                }
            }
        }
        progress.tick();
    });

    let failures = failures.into_inner().unwrap_or_default();
    Summary {
        ok: ok.load(Ordering::Relaxed),
        failed: failures.len(),
        seconds: (start.elapsed().as_secs_f64() * 100.0).round() / 100.0,
        failures,
    }
}

/// (source, destination, backup) triples. Images in `input` map into `output` and `backup`; images
/// in a direct subfolder map into a same-named subfolder of each (no deeper). A subfolder with no
/// images gets no counterpart. The backup keeps the original name; only the output becomes `.jpg`.
fn collect(input: &Path, output: &Path, backup: &Path) -> Vec<(PathBuf, PathBuf, PathBuf)> {
    let mut jobs = Vec::new();
    let Ok(entries) = std::fs::read_dir(input) else {
        return jobs;
    };

    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if is_supported(&path) {
            let dest = output.join(dest_name(&path));
            let kept = backup.join(source_name(&path));
            jobs.push((path, dest, kept));
        }
    }

    subdirs.sort();
    for dir in subdirs {
        let Some(name) = dir.file_name() else { continue };
        let Ok(sub_entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in sub_entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_supported(&path) {
                let dest = output.join(name).join(dest_name(&path));
                let kept = backup.join(name).join(source_name(&path));
                jobs.push((path, dest, kept));
            }
        }
    }

    jobs.sort();
    jobs
}

fn is_supported(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| SUPPORTED.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
}

fn dest_name(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    format!("{stem}.jpg")
}

/// The original's own file name, kept as-is for the backup copy.
fn source_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| dest_name(path))
}
