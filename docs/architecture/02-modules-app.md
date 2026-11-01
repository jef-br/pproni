# 2. Modules: `app`, the host shell

[Up: table of contents](TOC.md)

| Module | Surface | Notes |
|---|---|---|
| `app::batch` | `run(input, output, backup, footer, lang) -> Summary`, `Summary` | rayon fan-out, one level of subfolder recursion; drives `progress` |
| `app::progress` | `Progress`, `new(total, footer, lang)`, `tick` | in-place unicode bar, always 50 cells with the `NN%` drawn over them; throttled ~15 Hz via non-blocking `try_lock`; self-disables when stdout is not a TTY |
| `app::shell` | `run()` | Windows double-click flow: desktop `PPRONI-INPUT` → `PPRONI-OUTPUT`, originals → `PPRONI-BACKUP`. Draws the three screens |
| `app::header` | `print()` | Windows-only; hand-tuned banner (block logo + name + tagline), same on all three screens |
| `app::theme` | `WIDTH`, the palette, `rule`, `callout`, `slug` | 24-bit ANSI matching the design hex; owns the frame every screen is drawn in |
| `app::console` | `init`, `clear`, `ui_language`, `folder_url`, `pause` | Windows-only; ANSI, OSC-8 links, UI language |
| `app::desktop` | `desktop_dir()` | Windows-only |
| `app::i18n` | `Lang`, `drop_prompt`, `close_line`, `interrupt_hint`, `backup_label`, `output_label`, `remaining`, `finished` | EN/ES/FR/NL/IT/DE |

`console`, `desktop`, `header` and `shell` are `#[cfg(windows)]`; `progress` and `theme` are cross-platform. On other hosts only the two-argument CLI form
(`pproni <input_dir> <output_dir>`) is available, and `i18n` reads as dead code because nothing calls
it — that is expected, not drift.
