//! Batch-run the shot classifier (BiRefNet backend) over a folder of images. For every input it
//! writes into `<output_dir>` the input renamed with its partial shot code
//! (`<stem>--EIX=<trbl>[--BGC=<hex>][--FGC=<hex>].<ext>`), the segmask and, if an edge was gated,
//! the refine canvas; plus `zzz_results.json`. See docs/architecture/modules/examples.md.
//!
//! Usage:
//!   cargo run --release --example run_dir -- <input> [output_dir]
//!   <input> is a folder of images or a single image file.
//!
//! Model: the weights embedded in the exe by default. Set BIREFNET_ONNX to a path to run a different
//! export instead — that is what this harness is for. ORT_DYLIB_PATH overrides the ONNX Runtime
//! library (default: ./onnxruntime.dll).

use pproni::core::shot_classifier::geometry::Rect;
use pproni::core::shot_classifier::refine::{RefineParams, RefinementInput};
use pproni::core::exporter::{self, colors};
use pproni::core::shot_classifier::shotcode::eix_bits;
use pproni::core::shot_classifier::{classify_instance, BiRefNetConfig, BiRefNetModel, Edge, SegmentationModel};
use image::{Rgb, RgbImage};
use std::path::{Path, PathBuf};

const DEFAULT_OUTPUT: &str = "test data/OUTPUT";

// Snapshot name suffixes; each TAG must match its rectangle's green-bold word on
// `docs/diagrams/JBA2B.drawio.svg` verbatim. See docs/diagrams/run-dir-tag-protocol.md.
const SEGMASK_SUFFIX: &str = "___3-class_10_SEGMASK.png";
const REFINE_SUFFIX: &str = "___3-class_20_REFINE.png";

fn main() {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: run_dir <input> [output_dir]   (<input> = image folder or single image file)");
        std::process::exit(2);
    }));
    let output_dir = PathBuf::from(args.next().unwrap_or_else(|| DEFAULT_OUTPUT.into()));

    let model_path = std::env::var("BIREFNET_ONNX").ok();
    let ort_lib = std::env::var("ORT_DYLIB_PATH").unwrap_or_else(|_| "onnxruntime.dll".into());
    // Absolute path, or the OS search finds System32's older onnxruntime.dll first (version-mismatch
    // panic); `join` leaves an absolute `ORT_DYLIB_PATH` as is.
    // See docs/architecture/modules/examples.md.
    let ort_lib = std::env::current_dir()
        .map(|dir| dir.join(&ort_lib))
        .ok()
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or(ort_lib);

    let meta = std::fs::metadata(&input)
        .unwrap_or_else(|e| panic!("cannot open {}: {e}", input.display()));
    let mut entries: Vec<PathBuf> = if meta.is_dir() {
        std::fs::read_dir(&input)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", input.display()))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| is_image(p))
            .collect()
    } else {
        vec![input.clone()]
    };
    entries.sort();

    if entries.is_empty() {
        eprintln!("no images found in {}", input.display());
        std::process::exit(1);
    }

    std::fs::create_dir_all(&output_dir).expect("could not create output dir");
    // Only wipe OUTPUT for a full-folder run; a single-file run just
    // overwrites that image's own outputs.
    if meta.is_dir() {
        let cleared = clear_output_files(&input, &output_dir);
        if cleared > 0 {
            eprintln!("cleared {cleared} file(s) from {}", output_dir.display());
        }
    }

    BiRefNetModel::init_runtime(&ort_lib).expect("failed to load ONNX Runtime");
    let model = match &model_path {
        Some(path) => {
            eprintln!("loading BiRefNet from file: {path}");
            BiRefNetModel::load_from_file(path, BiRefNetConfig::default())
        }
        None => {
            eprintln!("loading the embedded BiRefNet");
            BiRefNetModel::load(BiRefNetConfig::default())
        }
    }
    .expect("failed to load BiRefNet model");

    let mut json_rows: Vec<String> = Vec::new();
    println!("\n{:<44}  {:>10}  {}", "image", "size", "shot code / touches");
    println!("{}", "-".repeat(96));

    for path in &entries {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("img");

        // Debug trace (PPRONI_DEBUG_TAGS marker file next to run_dir.exe): every stage image this
        // run produces, as `<stem>  -  NN.jpg`, written next to run_dir's output files.
        let trace_target = output_dir.join(&file_name);
        let _trace = pproni::core::trace::begin(&trace_target);

        let image = match image::open(path) {
            Ok(img) => img.to_rgb8(),
            Err(e) => {
                println!("{file_name:<44}  {:>10}  <open failed: {e}>", "-");
                continue;
            }
        };

        let instances = match model.segment(&image) {
            Ok(instances) => instances,
            Err(e) => {
                println!("{file_name:<44}  {:>10}  <segment failed: {e}>", "-");
                continue;
            }
        };
        let inst = &instances[0]; // BiRefNet returns exactly one whole-image instance

        let fg = inst.mask.data.iter().filter(|&&v| v > 0).count();
        let total = (image.width() * image.height()) as usize;

        let input = RefinementInput { working_image: &image, original_image: &image, instance: inst };
        let result = classify_instance(&input, 20, RefineParams::default());

        // --- shot code: BGC/FGC from the mask, EIX from the verdict (shared with the pipeline) ---
        let eix = eix_bits(&result.touches_edges);
        let sampled = colors::sample(&image, &inst.mask);
        let colors::ShotColors { bgc, fgc } = &sampled;
        let mask_too_small = &result.mask_too_small;
        let fg_ratio = fg as f64 / total as f64;

        let edges: Vec<&str> = result.touches_edges.iter().map(edge_name).collect();
        let verdict = if edges.is_empty() { "none".to_string() } else { edges.join(", ") };
        let code = format!(
            "EIX={eix}{}{}",
            bgc.as_deref().map(|c| format!(" BGC={c}")).unwrap_or_default(),
            fgc.as_deref().map(|c| format!(" FGC={c}")).unwrap_or_default(),
        );
        println!(
            "{file_name:<44}  {:>10}  {code}   [{verdict}{}]",
            format!("{}x{}", image.width(), image.height()),
            if *mask_too_small { " · full-bleed" } else { "" },
        );
        for r in &result.refinements {
            println!("    gate flagged {:<6} -> refined: touching={}", edge_name(&r.edge), r.touching);
        }

        // step 0: the input image, verbatim, renamed with the shot code.
        let tagged_name = format!("{stem}{}.{ext}", exporter::tags(&eix, &sampled));
        let _ = std::fs::copy(path, output_dir.join(&tagged_name));

        // step 1: BiRefNet foreground mask, full frame.
        let mut m = image::GrayImage::new(inst.mask.width, inst.mask.height);
        for (i, px) in m.pixels_mut().enumerate() {
            px.0[0] = inst.mask.data[i];
        }
        let _ = m.save(output_dir.join(format!("{stem}{SEGMASK_SUFFIX}")));

        // step 2: one canvas the size of the segmask, white except for
        // the refined region of each gated edge, pasted back where it
        // sits in the frame (top region up top, bottom down low, etc.).
        let regions: Vec<Rect> = result
            .refinements
            .iter()
            .map(|r| r.crop_region)
            .filter(|r| r.w > 0 && r.h > 0)
            .collect();
        if !regions.is_empty() {
            let mut canvas =
                RgbImage::from_pixel(inst.mask.width, inst.mask.height, Rgb([255, 255, 255]));
            for reg in &regions {
                let patch = crop_region(&image, *reg);
                image::imageops::overlay(&mut canvas, &patch, reg.x as i64, reg.y as i64);
            }
            let _ = canvas.save(output_dir.join(format!("{stem}{REFINE_SUFFIX}")));
        }

        let gate_json: Vec<String> = result
            .refinements
            .iter()
            .map(|r| format!("{{\"edge\":\"{}\",\"touching\":{}}}", edge_name(&r.edge), r.touching))
            .collect();
        let opt_str = |o: &Option<String>| o.as_deref().map(|s| format!("{s:?}")).unwrap_or_else(|| "null".into());
        json_rows.push(format!(
            "  {{\"image\":{:?},\"width\":{},\"height\":{},\"foreground_ratio\":{:.4},\
             \"eix\":{:?},\"mask_too_small\":{},\"bgc\":{},\"fgc\":{},\"touches_edges\":[{}],\"gate\":[{}]}}",
            file_name,
            image.width(),
            image.height(),
            fg_ratio,
            eix,
            mask_too_small,
            opt_str(bgc),
            opt_str(fgc),
            edges.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>().join(","),
            gate_json.join(",")
        ));
    }

    let json = format!("[\n{}\n]\n", json_rows.join(",\n"));
    // Folder run -> the aggregate `zzz_results.json`; single-file run ->
    // `<stem>_results.json`, so it never clobbers a folder run's report.
    let json_name = if meta.is_dir() {
        "zzz_results.json".to_string()
    } else {
        format!("{}_results.json", input.file_stem().unwrap().to_string_lossy())
    };
    let json_path = output_dir.join(json_name);
    std::fs::write(&json_path, json).expect("could not write results json");

    println!("\nwrote {}", json_path.display());
    println!("images written to {}", output_dir.display());
}

fn crop_region(image: &RgbImage, r: Rect) -> RgbImage {
    image::imageops::crop_imm(image, r.x, r.y, r.w.max(1), r.h.max(1)).to_image()
}

fn is_image(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp" | "bmp")
    )
}

/// Deletes regular files sitting directly in `output_dir` so a re-run
/// doesn't leave stale crops from a previous verdict. Subdirectories are
/// left untouched, and it refuses if output and input are the same folder.
fn clear_output_files(input_dir: &Path, output_dir: &Path) -> usize {
    let same = std::fs::canonicalize(input_dir).ok() == std::fs::canonicalize(output_dir).ok();
    if same {
        return 0;
    }
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(output_dir) {
        for entry in rd.flatten() {
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) && std::fs::remove_file(entry.path()).is_ok() {
                n += 1;
            }
        }
    }
    n
}

fn edge_name(e: &Edge) -> &'static str {
    match e {
        Edge::Top => "top",
        Edge::Bottom => "bottom",
        Edge::Left => "left",
        Edge::Right => "right",
    }
}
