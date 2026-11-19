//! Per-run settings, built once before the batch and passed to every stage as `&Options`; never
//! changed after. See docs/architecture/modules/core.md.

/// Every field defaults to the behaviour of a run without the setting.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Headcutting for ALL images. Off by default. See docs/work-state/2026-10-28-headcuts.md.
    pub enable_headcuts: bool,
}
