//! Remote terminal UI child process.
//!
//! This module is intentionally split into small files: session/socket orchestration,
//! terminal rendering, input parsing, selectors, panels, labels, render adapters, and logging.

mod input;
mod labels;
mod logging;
mod panels;
mod render;
mod selectors;
mod session;
mod snapshot;
mod tui;

pub use session::run;
