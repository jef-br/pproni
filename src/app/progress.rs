//! Throttled, in-place batch progress bar with a fixed footer under it.
//!
//! `batch::run` fans work across cores with rayon and calls [`Progress::tick`] once per finished
//! file from whatever worker thread finished it. Redraws are throttled to ~15 Hz through a
//! non-blocking `try_lock`: a worker that can't take the gate just bumps the counter and returns,
//! so the bar never sits on the batch's critical path. The bar disables itself when stdout is not
//! a terminal, which keeps it out of the piped `<exe> <in> <out>` dev path.
//!
//! The footer is the block the shell wants held under the bar — the rule / callout / rule that
//! tells the user how to interrupt. It is drawn once, with the first frame, and then left alone:
//! only the bar line ever changes, so every later frame steps the cursor up to that one line,
//! rewrites it, and steps back down. Nothing else writes to stdout while the batch runs (failures
//! are collected, not printed), so the block stays where it was first drawn.
//!
//! Each frame goes out in a single `write_all`. Rust's stdout is a `LineWriter` and flushes on
//! every newline it is handed, so printing the block line by line put a half-drawn frame on screen
//! fifteen times a second — which the terminal is free to composite. That was the flicker.

use super::i18n::{self, Lang};
use super::theme::{self, BOLD, RESET, TEAL};
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cells in the bar. The bar is *always* this wide: the percentage is drawn over the cells it
/// needs rather than appended, so the block below it never shifts.
const WIDTH: usize = 50;
const FULL: char = '\u{2588}'; // █  done
const EMPTY: char = '\u{2591}'; // ░ remaining — same teal, sparser glyph
const MIN_REDRAW: Duration = Duration::from_millis(66); // ~15 Hz
/// Clear the rest of the line the cursor sits on — the bar line keeps its width, but the
/// `remaining` readout beside it disappears on the closing frame.
const CLEAR_EOL: &str = "\x1b[K";

pub struct Progress {
    total: usize,
    done: AtomicUsize,
    start: Instant,
    /// Guards stdout and holds the last render time. `try_lock` is the throttle.
    gate: Mutex<Instant>,
    enabled: bool,
    /// Lines held under the bar, repainted with it.
    footer: Vec<String>,
    /// UI language, for the `remaining` readout beside the bar.
    lang: Lang,
}

impl Progress {
    /// Draw the first frame — bar plus `footer` — and hand back a handle the workers tick. Pass an
    /// empty `footer` for a bar with nothing beneath it.
    pub fn new(total: usize, footer: &[String], lang: Lang) -> Progress {
        let progress = Progress {
            total,
            done: AtomicUsize::new(0),
            start: Instant::now(),
            // Start in the past so the first tick draws immediately.
            gate: Mutex::new(Instant::now() - MIN_REDRAW),
            enabled: total > 0 && std::io::stdout().is_terminal(),
            footer: footer.to_vec(),
            lang,
        };
        if progress.enabled {
            let mut out = std::io::stdout().lock();
            progress.paint(&mut out, 0, false);
        }
        progress
    }

    /// Record one finished file (success or failure) and redraw if the throttle allows.
    pub fn tick(&self) {
        if !self.enabled {
            return;
        }
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;

        if done >= self.total {
            // Final frame always draws, throttle or not.
            if let Ok(mut last) = self.gate.lock() {
                self.redraw(self.total);
                *last = Instant::now();
            }
            return;
        }

        if let Ok(mut last) = self.gate.try_lock() {
            if last.elapsed() >= MIN_REDRAW {
                self.redraw(done);
                *last = Instant::now();
            }
        }
    }

    fn redraw(&self, done: usize) {
        let mut out = std::io::stdout().lock();
        self.paint(&mut out, done, true);
    }

    /// Build the whole frame in memory and hand it to the terminal in one write.
    ///
    /// The opening frame (`rewind` false) prints the bar and the footer under it. Every frame after
    /// that touches the bar line alone: up to it, rewrite it, back down to where the cursor was.
    /// The footer is never erased, so the screen never holds a blank where it used to be.
    fn paint<W: Write>(&self, out: &mut W, done: usize, rewind: bool) {
        let bar = format!(
            "{}{}{}",
            " ".repeat(theme::INDENT),
            self.bar(done),
            self.label(done)
        );

        let frame = if rewind {
            let up = self.footer.len() + 1;
            format!("\x1b[{up}A\r{bar}{CLEAR_EOL}\x1b[{up}B\r")
        } else {
            let mut frame = format!("{bar}\n");
            for line in &self.footer {
                frame.push_str(line);
                frame.push('\n');
            }
            frame
        };

        let _ = out.write_all(frame.as_bytes());
        let _ = out.flush();
    }

    /// `  08m25s remaining`, sitting to the right of the bar. Empty on the first frame (no
    /// estimate yet) and on the closing one (nothing left to wait for). The percentage is not
    /// here — it lives inside the bar.
    fn label(&self, done: usize) -> String {
        if done == 0 || done >= self.total {
            return String::new();
        }
        let per = self.start.elapsed().as_secs_f64() / done as f64;
        let rem = (per * (self.total - done) as f64).round() as u64;
        format!(
            "  {TEAL}{:02}m{:02}s {}{RESET}",
            rem / 60,
            rem % 60,
            i18n::remaining(self.lang)
        )
    }

    /// The bar: `WIDTH` teal cells, solid up to `done` and hatched after it, with the percentage
    /// drawn *over* the cells it needs. Pinning the label's start at `WIDTH - len` keeps the bar
    /// exactly `WIDTH` wide at 100%, where it ends up sitting on top of the filled cells.
    fn bar(&self, done: usize) -> String {
        bar_line(TEAL, done as f64 / self.total.max(1) as f64)
    }
}

/// The bar line at 0% in the slug's dark grey, indented like the live one — the idle bar shown
/// behind the headcut question.
pub fn dimmed_empty_bar() -> String {
    format!("{}{}", " ".repeat(theme::INDENT), bar_line(theme::DARK, 0.0))
}

/// `WIDTH` cells in `colour`, solid up to `frac`, with the percentage drawn over them.
fn bar_line(colour: &str, frac: f64) -> String {
    let filled = ((frac * WIDTH as f64).round() as usize).min(WIDTH);
    let cells: Vec<char> = (0..WIDTH)
        .map(|i| if i < filled { FULL } else { EMPTY })
        .collect();

    let pct = format!("{}%", (frac * 100.0).round() as u32);
    let len = pct.chars().count().min(WIDTH);
    let at = filled.min(WIDTH - len);
    format!(
        "{colour}{}{BOLD}{pct}{RESET}{colour}{}{RESET}",
        cells[..at].iter().collect::<String>(),
        cells[at + len..].iter().collect::<String>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Progress` that never touches stdout, for exercising [`Progress::bar`] alone.
    fn silent(total: usize) -> Progress {
        silent_with_footer(total, Vec::new())
    }

    /// As [`silent`], with a footer under the bar — what [`Progress::paint`] has to leave alone.
    fn silent_with_footer(total: usize, footer: Vec<String>) -> Progress {
        Progress {
            total,
            done: AtomicUsize::new(0),
            start: Instant::now(),
            gate: Mutex::new(Instant::now()),
            enabled: false,
            footer,
            lang: Lang::En,
        }
    }

    /// Printed cells, ignoring the SGR escapes woven through the bar.
    fn visible(s: &str) -> usize {
        let mut cells = 0;
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                cells += 1;
            }
        }
        cells
    }

    /// The opening frame is the only one that draws the footer, and it draws it once.
    #[test]
    fn first_frame_draws_the_footer() {
        let footer = vec![String::new(), "RULE".into(), "CALLOUT".into()];
        let mut out = Vec::new();
        silent_with_footer(10, footer).paint(&mut out, 0, false);
        let frame = String::from_utf8(out).unwrap();

        assert_eq!(frame.matches("RULE").count(), 1);
        assert_eq!(frame.matches("CALLOUT").count(), 1);
        assert_eq!(frame.lines().count(), 4);
    }

    /// A repaint rewrites the bar line and nothing else. A newline in this frame would walk the
    /// footer down the screen a row per frame; reprinting the footer would blank it fifteen times a
    /// second, which is what the flicker was.
    #[test]
    fn repaint_touches_the_bar_line_only() {
        let footer = vec![String::new(), "RULE".into(), "CALLOUT".into()];
        let mut out = Vec::new();
        silent_with_footer(10, footer).paint(&mut out, 5, true);
        let frame = String::from_utf8(out).unwrap();

        assert!(!frame.contains('\n'), "{frame:?}");
        assert!(
            !frame.contains("RULE") && !frame.contains("CALLOUT"),
            "{frame:?}"
        );
        // Four footer lines above the cursor plus the bar itself: up four, and back down four.
        assert!(frame.starts_with("\x1b[4A\r"), "{frame:?}");
        assert!(frame.ends_with("\x1b[4B\r"), "{frame:?}");
    }

    /// The frame under the bar only stays put if the bar never changes width — including at 100%,
    /// where the percentage has to be drawn over the filled cells rather than after them.
    #[test]
    fn bar_is_always_forty_cells() {
        for total in [1usize, 7, 37, 40, 100, 999] {
            for done in 0..=total {
                let bar = silent(total).bar(done);
                assert_eq!(visible(&bar), WIDTH, "total={total}, done={done}");
            }
        }
    }
}
