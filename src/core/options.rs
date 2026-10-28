//! Per-run settings. The frontend builds one `Options` before the batch starts; `core::process_file`
//! then hands the same `&Options` to every stage as a plain argument. Nothing changes it after
//! that, and no stage reads it from anywhere else.

/// Every field defaults to the behaviour of a run without the setting.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Headcutting for ALL images. Off by default. Nothing reads it yet; what it does is decided
    /// later.
    pub enable_headcuts: bool,
}
