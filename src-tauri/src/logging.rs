//! Minimal file log. A tray app has no console, so a crash at 3am must leave
//! something readable behind. One file, rotated once at 5 MB.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

const MAX_BYTES: u64 = 5 * 1024 * 1024;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init(path: PathBuf) {
    let _ = LOG_PATH.set(path);
}

pub fn log(message: impl AsRef<str>) {
    let line = format!(
        "[{}] {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        message.as_ref()
    );

    if let Some(path) = LOG_PATH.get() {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > MAX_BYTES {
                let _ = std::fs::remove_file(path.with_extension("log.1"));
                let _ = std::fs::rename(path, path.with_extension("log.1"));
            }
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(line.as_bytes());
        }
    }

    #[cfg(debug_assertions)]
    eprint!("{line}");
}
