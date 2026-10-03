//! The app log: details behind the plain-language errors the user sees (AGENTS.md
//! section 8). A small rotating file in the app's log folder: `folio.log`, rotated at
//! 1 MiB, keeping three older files. Nothing leaves the machine.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1024 * 1024;
const KEEP: usize = 3;

struct Log {
    dir: PathBuf,
    file: Option<File>,
    size: u64,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

/// Starts logging into `dir`. Until this is called (and if the folder cannot be created),
/// messages go to stderr only.
pub fn init(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let log = Log {
        dir: dir.to_path_buf(),
        file: None,
        size: 0,
    };
    let _ = LOG.set(Mutex::new(log));
}

#[derive(Debug, Clone, Copy)]
pub enum Level {
    Info,
    Warn,
    Error,
}

pub fn info(message: impl AsRef<str>) {
    write(Level::Info, message.as_ref());
}

pub fn warn(message: impl AsRef<str>) {
    write(Level::Warn, message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    write(Level::Error, message.as_ref());
}

fn write(level: Level, message: &str) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let line = format!("{secs:.3} {level:?} {message}\n");
    if cfg!(debug_assertions) {
        eprint!("[folio] {line}");
    }
    let Some(log) = LOG.get() else {
        return;
    };
    // A poisoned lock only means another thread panicked mid-write; logging can go on.
    let mut log = log.lock().unwrap_or_else(|e| e.into_inner());
    // Logging must never fail the operation being logged, so errors are ignored here.
    let _ = log.append(line.as_bytes());
}

impl Log {
    fn path(&self, index: usize) -> PathBuf {
        if index == 0 {
            self.dir.join("folio.log")
        } else {
            self.dir.join(format!("folio.{index}.log"))
        }
    }

    fn append(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if self.file.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.path(0))?;
            self.size = file.metadata()?.len();
            self.file = Some(file);
        }
        if self.size + bytes.len() as u64 > MAX_BYTES {
            self.rotate()?;
            return self.append(bytes);
        }
        if let Some(file) = self.file.as_mut() {
            file.write_all(bytes)?;
            self.size += bytes.len() as u64;
        }
        Ok(())
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        self.file = None;
        let _ = fs::remove_file(self.path(KEEP));
        for i in (0..KEEP).rev() {
            let from = self.path(i);
            if from.exists() {
                fs::rename(&from, self.path(i + 1))?;
            }
        }
        self.size = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_and_keeps_three_old_files() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-output/applog");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut log = Log {
            dir: dir.clone(),
            file: None,
            size: 0,
        };
        let chunk = vec![b'x'; 300 * 1024];
        for _ in 0..20 {
            log.append(&chunk).unwrap();
        }
        assert!(log.path(0).exists());
        assert!(log.path(3).exists());
        assert!(!log.path(4).exists());
        assert!(fs::metadata(log.path(0)).unwrap().len() <= MAX_BYTES);
    }
}
