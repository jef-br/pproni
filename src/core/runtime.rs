//! The ONNX Runtime shared library, carried inside the executable.
//!
//! Pproni ships as one file. The BiRefNet weights are already compiled in
//! (`shot_classifier::birefnet`); this module does the same for
//! `onnxruntime.dll`, the last file that used to sit beside the exe.
//!
//! `ort`'s `load-dynamic` needs a real path to `LoadLibrary`, so the
//! embedded copy is written out once to a per-user cache directory
//! (`%LOCALAPPDATA%`, falling back to the temp dir) and the path to that
//! copy is handed to `ort::init_from`. The extraction is
//! keyed on the library's size + a content sample: a rebuild against a
//! different runtime lands in a different directory, and repeat runs reuse
//! the file already there.
//!
//! This exists only while the runtime cannot be linked in — see
//! `Cargo.toml`'s `ort` entry and `docs/ARCHITECTURE.md` §5. When a static
//! ORT >= 1.22 is available, this module and `birefnet::init_runtime` both
//! delete outright.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The DirectML build of ONNX Runtime, compiled into the executable.
///
/// Gitignored and expected at the repo root, exactly like the `.onnx`
/// weights: a missing file is a *build* failure naming it, not a runtime
/// surprise. This is the build that can run the model on a graphics card;
/// it also runs it on the CPU, so it is the only runtime Pproni carries.
static DYLIB_BYTES: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/onnxruntime-directml.dll"));

/// The DirectML library the runtime above hands the model to. Extracted beside it.
static DIRECTML_BYTES: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/DirectML.dll"));

const DYLIB_NAME: &str = "pproni.dll";
/// DirectML is looked up by this exact name, so it keeps it.
const DIRECTML_NAME: &str = "DirectML.dll";

/// Every file the runtime needs on disk, as (name, bytes).
fn embedded_files() -> [(&'static str, &'static [u8]); 2] {
    [(DYLIB_NAME, DYLIB_BYTES), (DIRECTML_NAME, DIRECTML_BYTES)]
}

/// Path to an `onnxruntime.dll` on disk that `ort` can load.
///
/// `ORT_DYLIB_PATH` wins when it points at a real file — the `examples/`
/// dev harness sets it to run against a hand-placed runtime. Otherwise the
/// embedded copy is materialised in a temp directory and that path is
/// returned.
pub fn onnxruntime_dylib() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("ORT_DYLIB_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    let dylib = extract_embedded()?;
    if let Some(dir) = dylib.parent() {
        preload_directml(dir);
    }
    Ok(dylib)
}

/// Loads our `DirectML.dll` into the process by full path, before the runtime asks for it by name.
///
/// Windows hands a later request for the bare name `DirectML.dll` the copy already loaded, so the
/// runtime gets the version we shipped, not whatever older one sits in System32. A failure is not
/// fatal and not reported: the graphics-card route then fails to start and the model runs on the
/// CPU. The handle is deliberately never released.
#[cfg(windows)]
fn preload_directml(dir: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;

    let wide: Vec<u16> = dir.join(DIRECTML_NAME).as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        LoadLibraryW(wide.as_ptr());
    }
}

#[cfg(not(windows))]
fn preload_directml(_dir: &Path) {}

/// Write the embedded files to `<base>/pproni-<tag>/`, reusing any that are
/// already there at the right size.
///
/// `<base>` is the first of `%LOCALAPPDATA%`, the system temp dir, and the
/// executable's own directory that we can actually write into — so a machine
/// with an unset or read-only `TEMP` still starts.
fn extract_embedded() -> Result<PathBuf, String> {
    let subdir = format!("pproni-{}", cache_tag());
    let mut last_err = String::from("no writable directory for the ONNX Runtime");

    'bases: for base in candidate_bases() {
        let dir = base.join(&subdir);

        let present = |name: &str, bytes: &[u8]| {
            fs::metadata(dir.join(name)).map(|m| m.len()).ok() == Some(bytes.len() as u64)
        };
        if embedded_files().iter().all(|(name, bytes)| present(name, bytes)) {
            return Ok(dir.join(DYLIB_NAME));
        }
        if let Err(e) = fs::create_dir_all(&dir).map_err(|e| e.to_string()) {
            last_err = format!("create {}: {e}", dir.display());
            continue;
        }
        for (name, bytes) in embedded_files() {
            if present(name, bytes) {
                continue;
            }
            if let Err(e) = write_atomically(&dir, name, bytes) {
                last_err = e;
                continue 'bases;
            }
        }
        return Ok(dir.join(DYLIB_NAME));
    }
    Err(last_err)
}

/// Directories to try as the extraction root, best first.
fn candidate_bases() -> Vec<PathBuf> {
    let mut bases = Vec::new();
    if let Some(p) = std::env::var_os("LOCALAPPDATA") {
        bases.push(PathBuf::from(p));
    }
    bases.push(std::env::temp_dir());
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))
    {
        bases.push(dir);
    }
    bases
}

/// Write to a uniquely named sibling, then rename into place. A half-written
/// file never carries the final name, and concurrent Pproni processes racing on
/// the same target are harmless — the loser sees the winner's finished file.
fn write_atomically(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let dest = dir.join(name);
    let tmp = dir.join(format!("{}.{}.tmp", name, std::process::id()));

    let mut f = fs::File::create(&tmp)
        .map_err(|e| format!("create {}: {e}", tmp.display()))?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| format!("write {}: {e}", tmp.display()))?;
    drop(f);

    match fs::rename(&tmp, &dest) {
        Ok(()) => Ok(()),
        Err(_) if fs::metadata(&dest).map(|m| m.len()).ok() == Some(bytes.len() as u64) =>
        {
            let _ = fs::remove_file(&tmp);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(format!("rename into {}: {e}", dest.display()))
        }
    }
}

/// A short hex tag identifying this exact runtime build: its length plus an
/// FNV-1a hash of the head, middle and tail. Cheap to compute, and distinct
/// whenever the embedded DLL changes.
fn cache_tag() -> String {
    const SAMPLE: usize = 4096;
    let n = DYLIB_BYTES.len();

    let mut h: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    feed(&(n as u64).to_le_bytes());
    feed(&(DIRECTML_BYTES.len() as u64).to_le_bytes());
    feed(&DYLIB_BYTES[..SAMPLE.min(n)]);
    feed(&DYLIB_BYTES[n / 2..(n / 2 + SAMPLE).min(n)]);
    feed(&DYLIB_BYTES[n.saturating_sub(SAMPLE)..]);

    format!("{n:x}-{h:016x}")
}
