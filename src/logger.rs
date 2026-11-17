//! Structured Debug Logger for Open MusicFlow Studio (Rust Edition).
//! Outputs to stdout, `/tmp/open-musicflow-debug.log`, `~/Library/Logs/OpenMusicFlowStudio/debug.log` (macOS),
//! and an in-memory ring buffer exposed at `GET /api/debug-log`.

use crate::protocol::iso_timestamp_now;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LOG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn buffer() -> &'static Mutex<Vec<String>> {
    LOG_BUFFER.get_or_init(|| Mutex::new(Vec::with_capacity(512)))
}

pub fn debug_log_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/tmp/open-musicflow-debug.log")];
    if let Ok(home) = env::var("HOME") {
        let macos_logs = PathBuf::from(&home).join("Library/Logs");
        if macos_logs.exists() {
            let dir = macos_logs.join("OpenMusicFlowStudio");
            let _ = fs::create_dir_all(&dir);
            paths.push(dir.join("debug.log"));
        }
    }
    paths
}

pub fn log_debug(component: &str, message: &str) {
    let ts = iso_timestamp_now();
    let line = format!("[{}][{}] {}", ts, component, message);

    // 1. Print to stdout for live terminal debugging
    println!("{}", line);

    // 2. Append to persistent log files (/tmp/open-musicflow-debug.log & ~/Library/Logs/OpenMusicFlowStudio/debug.log)
    for path in debug_log_paths() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{}", line);
        }
    }

    // 3. Keep in-memory ring buffer for GET /api/debug-log
    if let Ok(mut guard) = buffer().lock() {
        guard.push(line);
        if guard.len() > 500 {
            let excess = guard.len() - 500;
            guard.drain(0..excess);
        }
    }
}

pub fn get_recent_logs_text() -> String {
    if let Ok(guard) = buffer().lock() {
        guard.join("\n")
    } else {
        String::new()
    }
}
