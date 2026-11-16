//! A timing trace for tuning, off unless `TABREF_TRACE` names a file. A
//! command marks its phases; when it ends, one line is appended to that
//! file: the command and each phase's milliseconds since the command began,
//! `open navigated=812 downloads=813 snapshot=1702 …`. The daemon traces
//! when it was started with the variable set. Nothing is printed and no
//! output changes.

use std::cell::RefCell;
use std::io::Write;
use std::time::Instant;

const VARIABLE: &str = "TABREF_TRACE";

struct Trace {
    command: String,
    started: Instant,
    marks: Vec<(String, u128)>,
}

thread_local! {
    static CURRENT: RefCell<Option<Trace>> = const { RefCell::new(None) };
}

/// Whether this process traces.
pub(crate) fn on() -> bool {
    std::env::var_os(VARIABLE).is_some_and(|path| !path.is_empty())
}

/// Starts the trace of `command` on this thread, when tracing is on.
pub(crate) fn begin(command: &str) {
    if !on() {
        return;
    }
    CURRENT.with(|current| {
        *current.borrow_mut() = Some(Trace {
            command: command.to_owned(),
            started: Instant::now(),
            marks: Vec::new(),
        });
    });
}

/// Marks the end of phase `label` of the command being traced.
pub(crate) fn mark(label: &str) {
    CURRENT.with(|current| {
        if let Some(trace) = current.borrow_mut().as_mut() {
            let at = trace.started.elapsed().as_millis();
            trace.marks.push((label.to_owned(), at));
        }
    });
}

/// Adds `label=value` to the trace as it stands, for a figure that is not a
/// time of this process, such as the page's own load timings.
pub(crate) fn note(label: &str, value: u128) {
    CURRENT.with(|current| {
        if let Some(trace) = current.borrow_mut().as_mut() {
            trace.marks.push((label.to_owned(), value));
        }
    });
}

/// Ends the command's trace and appends its line to the trace file. A file
/// that cannot be written is ignored.
pub(crate) fn end() {
    let Some(trace) = CURRENT.with(|current| current.borrow_mut().take()) else {
        return;
    };
    let Some(path) = std::env::var_os(VARIABLE) else {
        return;
    };
    let mut line = trace.command.clone();
    for (label, at) in &trace.marks {
        line.push_str(&format!(" {label}={at}"));
    }
    line.push_str(&format!(" end={}\n", trace.started.elapsed().as_millis()));
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}
