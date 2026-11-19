//! Console output helpers: VT setup, screen clearing, clickable folder URLs, UI-language
//! detection, and a "press any key" pause so the window stays open on a double-click launch.

use super::i18n::Lang;
use std::path::Path;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    STD_OUTPUT_HANDLE,
};

/// Enable ANSI/virtual-terminal processing so colours and hyperlinks render (Windows 10+).
pub fn init() {
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0u32;
        if GetConsoleMode(handle, &mut mode) != 0 {
            SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

/// Wipe the screen, cursor to top-left: each screen replaces the last instead of scrolling.
pub fn clear() {
    print!("\x1b[2J\x1b[H");
}

/// Detect the user's UI language, mapping the primary language id to a supported `Lang`.
pub fn ui_language() -> Lang {
    let langid = unsafe { GetUserDefaultUILanguage() };
    let code = match langid & 0x03ff {
        0x0a => "es",
        0x0c => "fr",
        0x13 => "nl",
        0x10 => "it",
        0x07 => "de",
        _ => "en",
    };
    Lang::from_code(code)
}

/// A `file://` URL for `path` under Windows Terminal (OSC-8 links). `None` on classic conhost, so
/// it gets a plain path and no stray escape.
pub fn folder_url(path: &Path) -> Option<String> {
    if std::env::var_os("WT_SESSION").is_none() {
        return None;
    }
    let s = path.display().to_string().replace('\\', "/").replace(' ', "%20");
    Some(format!("file:///{s}"))
}

#[link(name = "msvcrt")]
extern "C" {
    fn _getch() -> i32;
}

/// Block until the user presses any key (no Enter required).
pub fn pause() {
    unsafe {
        let _ = _getch();
    }
}

/// Read lines until one is `Y`/`y` (true) or `N`/`n` (false); anything else asks again. A closed
/// stdin answers no.
pub fn ask_yes_no() -> bool {
    loop {
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(n) if n > 0 => match line.trim() {
                "Y" | "y" => return true,
                "N" | "n" => return false,
                _ => {}
            },
            _ => return false,
        }
    }
}
