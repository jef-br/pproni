# 2. Modules: `app`, the host shell

[Up: table of contents](TOC-architecture.md)

| Module | Surface | Notes |
|---|---|---|
| `app` | `run_default()` | the no-argument entry; Windows: `shell::run`, other hosts: usage line |
| `app::batch` | `run(input, output, backup, footer, lang, options) -> Summary`, `Summary` | rayon fan-out, one level of subfolder recursion; drives `progress` |
| `app::progress` | `Progress`, `new(total, footer, lang)`, `tick`, `dimmed_empty_bar` | in-place unicode bar, always 50 cells with the `NN%` drawn over them; throttled ~15 Hz via non-blocking `try_lock`; self-disables when stdout is not a TTY |
| `app::shell` | `run()` | Windows double-click flow: desktop `PPRONI-INPUT` → `PPRONI-OUTPUT`, originals → `PPRONI-BACKUP`. Draws the three screens; a fourth, the dimmed headcut question, only when `config::PERMIT_HEADCUTS` is on |
| `app::header` | `print(dim)` | Windows-only; same banner (block logo + name + tagline) on every screen, dimmed behind the headcut question |
| `app::theme` | `WIDTH`, `INDENT`, the palette, `rule`, `callout`, `question_callout`, `answer_column`, `slug` | 24-bit ANSI matching the design hex; owns the frame every screen is drawn in |
| `app::console` | `init`, `clear`, `ui_language`, `folder_url`, `pause`, `ask_yes_no` | Windows-only; ANSI, OSC-8 links, UI language |
| `app::desktop` | `desktop_dir()` | Windows-only |
| `app::i18n` | `Lang`, `Lang::from_code`, `drop_prompt`, `headcuts_question`, `close_line`, `interrupt_hint`, `backup_label`, `output_label`, `remaining`, `finished` | EN/ES/FR/NL/IT/DE |

`console`, `desktop`, `header` and `shell` are `#[cfg(windows)]`; the rest builds on any host, but Pproni ships for Windows only. The command-line form `pproni <input_dir> <output_dir> [backup_dir] [--enable-headcuts]` works everywhere and is the dev/test path.