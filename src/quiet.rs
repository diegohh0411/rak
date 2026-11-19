//! A process-wide switch that silences the progress lines commands print to
//! stdout/stderr. The TUI turns it on so library code can't draw over the
//! screen; plain CLI runs leave it off.

use std::sync::atomic::{AtomicBool, Ordering};

static QUIET: AtomicBool = AtomicBool::new(false);

pub fn set(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}
