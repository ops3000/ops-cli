//! Structured output system for ops-cli.
//!
//! Provides verbosity-aware macros for all user-facing output.
//! Initialize once from main() with `output::init(verbosity)`.
//!
//! NOTE: Output from child processes spawned with Stdio::inherit()
//! (e.g., docker compose, rsync, interactive SSH) bypasses this system
//! and always appears on the terminal.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Quiet = 0,
    Normal = 1,
    Verbose = 2,
}

static VERBOSITY: OnceLock<Verbosity> = OnceLock::new();

/// Initialize the global verbosity level. Must be called once from main().
pub fn init(v: Verbosity) {
    VERBOSITY.set(v).expect("output::init called more than once");
}

/// Get the current verbosity level.
pub fn verbosity() -> Verbosity {
    *VERBOSITY.get().unwrap_or(&Verbosity::Normal)
}

/// Major phase header. Shown at Normal+.
#[macro_export]
macro_rules! o_step {
    ($($arg:tt)*) => {
        if $crate::output::verbosity() >= $crate::output::Verbosity::Normal {
            println!($($arg)*);
        }
    };
}

/// Indented info/detail line. Shown at Normal+.
#[macro_export]
macro_rules! o_detail {
    ($($arg:tt)*) => {
        if $crate::output::verbosity() >= $crate::output::Verbosity::Normal {
            println!($($arg)*);
        }
    };
}

/// Completion/success indicator. Shown at Normal+.
#[macro_export]
macro_rules! o_success {
    ($($arg:tt)*) => {
        if $crate::output::verbosity() >= $crate::output::Verbosity::Normal {
            println!($($arg)*);
        }
    };
}

/// Non-fatal warning. Shown at Normal+.
#[macro_export]
macro_rules! o_warn {
    ($($arg:tt)*) => {
        if $crate::output::verbosity() >= $crate::output::Verbosity::Normal {
            eprintln!($($arg)*);
        }
    };
}

/// Fatal error. Always shown.
#[macro_export]
macro_rules! o_error {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

/// Debug/verbose info. Shown at Verbose only.
#[macro_export]
macro_rules! o_debug {
    ($($arg:tt)*) => {
        if $crate::output::verbosity() >= $crate::output::Verbosity::Verbose {
            println!($($arg)*);
        }
    };
}

/// Interactive prompt. Always shown, uses print! (no newline).
#[macro_export]
macro_rules! o_print {
    ($($arg:tt)*) => {
        print!($($arg)*);
    };
}

/// Final result summary. Always shown (even in Quiet mode).
#[macro_export]
macro_rules! o_result {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}
