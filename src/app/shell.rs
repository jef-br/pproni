//! Default (double-click) flow: find the desktop input folder, process, report in the console.
//!
//! Three screens, one frame: banner and two status lines, a body — input folder (start), progress
//! bar (running) or output folder (finish) — then rule, callout, rule. Each clears the console
//! first, so it replaces the last instead of scrolling. `theme` owns every rule and colour.

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
    // Handed to `Progress`, not printed: it draws them under the bar and steps the cursor back over
    // them on every redraw.
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
    // The blank line above the bar, printed here: `Progress` draws from the bar down only.
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

    // Below the frame, so a clean run shows the screen as designed; the count is in "N skipped".
    for line in &summary.failures {
        println!("{}{line}{}", theme::DARK, theme::RESET);
    }
    console::pause();
}

/// The one question before the batch: the running screen dimmed to the slug's grey, an empty bar,
/// and the question on the callout line ("Close this window…" there) with a blinking block cursor
/// after it. The real running screen then redraws over this one.
fn ask_headcuts(status: &[String; 2], lang: i18n::Lang) -> bool {
    let question = i18n::headcuts_question(lang);
    draw_top(status, true);
    println!();
    println!("{}", progress::dimmed_empty_bar());
    println!();
    println!("{}", theme::rule());
    println!("{}", theme::question_callout(question));
    println!("{}", theme::rule());
    // Up two lines, to the column after the question. `ESC[1 q` = blinking block cursor; `ESC[0 q`
    // below restores the terminal's own.
    print!("\x1b[2A\x1b[{}G\x1b[1 q", theme::answer_column(question));
    let _ = std::io::stdout().flush();
    let answer = console::ask_yes_no();
    print!("\x1b[0 q");
    let _ = std::io::stdout().flush();
    answer
}

/// `drop_prompt` as owned strings, what [`open_screen`] takes (the other screens use `format!`).
fn drop_prompt_lines(lang: i18n::Lang) -> [String; 2] {
    i18n::drop_prompt(lang).map(str::to_owned)
}

/// Wipe the console and draw the top of the frame: banner, status lines, rule, and the blank line
/// the body sits on (here, as `Progress` draws the running body from the bar down). The caller
/// adds the body and [`print_footer`].
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

/// `path` on its own indented line, bold in `colour` (teal input, orange output), clickable under
/// Windows Terminal as a BEL-terminated OSC-8 link (it takes either terminator).
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

// True when `dir` holds an image directly or in a direct subfolder (batch.rs recurses one level).
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
