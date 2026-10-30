//! Default (double-click) flow: find the desktop input folder, process, report in the console.
//!
//! Three screens, one frame. Each opens with the banner and two status lines, closes with a rule,
//! a callout and a rule, and carries one body between them: the input folder (start), the progress
//! bar (running), or the output folder (finish). Each clears the console first, so a screen
//! replaces the one before it instead of scrolling under it. `theme` owns every rule and colour.

use super::{batch, console, desktop, header, i18n, progress, theme};
use pproni::core::options::Options;
use std::io::Write;
use std::path::Path;

const INPUT_FOLDER: &str = "PPRONI-INPUT";
const OUTPUT_FOLDER: &str = "PPRONI-OUTPUT";
const BACKUP_FOLDER: &str = "PPRONI-BACKUP";

/// Left margin of the folder line, matching the rules and the bar.
const INDENT: &str = "  ";

pub fn run() {
    console::init();
    let lang = console::ui_language();

    let desktop = desktop::desktop_dir();
    let input = desktop.join(INPUT_FOLDER);
    let output = desktop.join(OUTPUT_FOLDER);
    let backup = desktop.join(BACKUP_FOLDER);

    let created = !input.exists();
    if created {
        let _ = std::fs::create_dir_all(&input);
    }

    // First run / nothing to do: point the user at the input folder and wait.
    if created || !has_any_image(&input) {
        open_screen(&drop_prompt_lines(lang));
        print_folder(&input, theme::TEAL);
        println!();
        print_footer(i18n::close_line(lang));
        console::pause();
        return;
    }

    let status = [
        format!("{}{}", i18n::backup_label(lang), backup.display()),
        format!("{}{}", i18n::output_label(lang), output.display()),
    ];
    // The bar is drawn first and the callout under it stays put; `Progress` walks the cursor back
    // over these lines on every redraw, so they have to be handed over rather than printed here.
    let footer = vec![
        String::new(),
        theme::rule(),
        theme::callout(i18n::interrupt_hint(lang)),
        theme::rule(),
    ];

    let options = Options { enable_headcuts: pproni::core::config::PERMIT_HEADCUTS && ask_headcuts(&status, lang) };

    let _ = std::fs::create_dir_all(&output);
    let _ = std::fs::create_dir_all(&backup);

    open_screen(&status);
    // The blank line above the bar. `Progress` draws from the bar down and rewinds only over its
    // own block, so this one has to be printed here — it is never repainted.
    println!();

    let summary = batch::run(&input, &output, &backup, &footer, lang, &options);

    open_screen(&i18n::finished(
        lang,
        summary.seconds,
        summary.ok,
        summary.failed,
    ));
    print_folder(&output, theme::ORANGE);
    println!();
    print_footer(i18n::close_line(lang));

    // Below the frame, so a clean run shows the screen exactly as designed. The count is already
    // carried by the "N skipped" half of the summary.
    for line in &summary.failures {
        println!("{}{line}{}", theme::DARK, theme::RESET);
    }
    console::pause();
}

/// The one question before the batch. It is the running screen with everything dimmed to the
/// slug's grey and an empty bar, and the question takes the callout line where the running screen
/// says "Close this window…". A blinking block cursor waits right after it. The real running
/// screen then redraws over this one.
fn ask_headcuts(status: &[String; 2], lang: i18n::Lang) -> bool {
    let question = i18n::headcuts_question(lang);
    draw_top(status, true);
    println!();
    println!("{}", progress::dimmed_empty_bar());
    println!();
    println!("{}", theme::rule());
    println!("{}", theme::question_callout(question));
    println!("{}", theme::rule());
    // Back up two lines to the question and out to the column after it; `ESC[1 q` is a blinking
    // block cursor, and `ESC[0 q` below hands the cursor back to the terminal's own setting.
    print!("\x1b[2A\x1b[{}G\x1b[1 q", theme::answer_column(question));
    let _ = std::io::stdout().flush();
    let answer = console::ask_yes_no();
    print!("\x1b[0 q");
    let _ = std::io::stdout().flush();
    answer
}

/// `drop_prompt` as owned strings — the other two screens build their status lines with
/// `format!`, so [`open_screen`] takes `String`s and this one has to match.
fn drop_prompt_lines(lang: i18n::Lang) -> [String; 2] {
    i18n::drop_prompt(lang).map(str::to_owned)
}

/// Wipe the console and draw the top of the frame: banner, two status lines, closing rule, and
/// the blank line the body sits on. The blank belongs here rather than to each caller — the
/// running screen's body is drawn by `Progress`, which starts at the bar. The caller follows with
/// the body and [`print_footer`].
fn open_screen(status: &[String; 2]) {
    draw_top(status, false);
}

/// [`open_screen`], with `dim` painting the banner and status lines in the slug's grey.
fn draw_top(status: &[String; 2], dim: bool) {
    let colour = if dim { theme::DARK } else { theme::WHITE };
    console::clear();
    header::print(dim);
    for line in status {
        println!("{colour}{line}{}", theme::RESET);
    }
    println!("{}", theme::rule());
    println!();
}

/// The folder path on its own indented line, bold in `colour` and clickable under Windows
/// Terminal. `colour` differs per screen — teal for the input folder, orange for the output one.
/// The hyperlink is BEL-terminated OSC-8 — Windows Terminal takes either terminator.
fn print_folder(path: &Path, colour: &str) {
    let painted = format!("{}{}{}{}", theme::BOLD, colour, path.display(), theme::RESET);
    let line = match console::folder_url(path) {
        Some(url) => format!("\x1b]8;;{url}\x07{painted}\x1b]8;;\x07"),
        None => painted,
    };
    println!("{INDENT}{line}");
}

/// The bottom of the frame: a callout line (marker, `text`, version slug) between two rules.
fn print_footer(text: &str) {
    println!("{}", theme::rule());
    println!("{}", theme::callout(text));
    println!("{}", theme::rule());
    let _ = std::io::stdout().flush();
}

// True when `dir` has something for batch::run to process: an image directly inside it, or an
// image inside one of its direct subfolders (batch recurses exactly one level — see batch.rs).
fn has_any_image(dir: &Path) -> bool {
    has_direct_image(dir) || has_subfolder_image(dir)
}

fn has_direct_image(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in entries.flatten() {
        if let Some(ext) = e.path().extension().and_then(|x| x.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if ["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"].contains(&ext.as_str()) {
                return true;
            }
        }
    }
    false
}

fn has_subfolder_image(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.path().is_dir() && has_direct_image(&e.path()))
}
