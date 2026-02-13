use std::io::{IsTerminal, Write};

/// Whether stderr is connected to a terminal (cached once per process).
fn use_color() -> bool {
    std::io::stderr().is_terminal()
}

/// Print a cargo-style status line: a bold green action word, right-aligned
/// to 12 characters, followed by the message.
///
/// Output goes to stderr so that stdout stays clean for piped binary output.
pub fn status(action: &str, message: &str) {
    let mut stderr = std::io::stderr().lock();
    if use_color() {
        let _ = writeln!(stderr, "\x1b[1;32m{action:>12}\x1b[0m {message}");
    } else {
        let _ = writeln!(stderr, "{action:>12} {message}");
    }
}

/// Print an error message with a bold red `error:` prefix.
pub fn error(message: &str) {
    let mut stderr = std::io::stderr().lock();
    if use_color() {
        let _ = writeln!(stderr, "\x1b[1;31merror\x1b[0m: {message}");
    } else {
        let _ = writeln!(stderr, "error: {message}");
    }
}

/// Print a warning message with a bold yellow `warning:` prefix.
pub fn warn(message: &str) {
    let mut stderr = std::io::stderr().lock();
    if use_color() {
        let _ = writeln!(stderr, "\x1b[1;33mwarning\x1b[0m: {message}");
    } else {
        let _ = writeln!(stderr, "warning: {message}");
    }
}
