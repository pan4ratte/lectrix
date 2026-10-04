//! Remembered app state: recent files and the view position of each file (AGENTS.md
//! section 6.1). Stored as JSON in the app's local data folder, never in the PDF.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ipc::{Appearance, ViewState};

const MAX_RECENT: usize = 20;
const MAX_VIEWS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentEntry {
    pub path: PathBuf,
    /// Seconds since the Unix epoch.
    pub opened_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct StoredView {
    view: ViewState,
    saved_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    recent: Vec<RecentEntry>,
    /// Keyed by `Platform::file_key`.
    #[serde(default)]
    views: HashMap<String, StoredView>,
    #[serde(default)]
    settings: StoredSettings,
}

/// What the Settings dialog changes (section 6.5).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StoredSettings {
    /// The author name for new annotations; `None` uses the Windows user name.
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub appearance: Appearance,
}

pub struct Store {
    file: Option<PathBuf>,
    data: Data,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Store {
    /// Loads `file`, or starts empty if it does not exist or cannot be read.
    pub fn load(file: Option<PathBuf>) -> Store {
        let data = file
            .as_deref()
            .and_then(|f| fs::read(f).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Store { file, data }
    }

    pub fn recent(&self) -> &[RecentEntry] {
        &self.data.recent
    }

    /// Moves `path` to the top of the recent list.
    pub fn add_recent(&mut self, path: &Path, same_file: impl Fn(&Path, &Path) -> bool) {
        self.data.recent.retain(|e| !same_file(&e.path, path));
        self.data.recent.insert(
            0,
            RecentEntry {
                path: path.to_path_buf(),
                opened_at: now(),
            },
        );
        self.data.recent.truncate(MAX_RECENT);
        self.persist();
    }

    pub fn remove_recent(&mut self, index: usize) {
        if index < self.data.recent.len() {
            self.data.recent.remove(index);
            self.persist();
        }
    }

    pub fn settings(&self) -> &StoredSettings {
        &self.data.settings
    }

    pub fn set_settings(&mut self, settings: StoredSettings) {
        if self.data.settings != settings {
            self.data.settings = settings;
            self.persist();
        }
    }

    pub fn view(&self, key: &str) -> Option<ViewState> {
        self.data.views.get(key).map(|v| v.view.clone())
    }

    pub fn set_view(&mut self, key: String, view: ViewState) {
        if self.data.views.get(&key).is_some_and(|v| v.view == view) {
            return;
        }
        self.data.views.insert(
            key,
            StoredView {
                view,
                saved_at: now(),
            },
        );
        if self.data.views.len() > MAX_VIEWS {
            // Forget the files viewed longest ago.
            let mut by_age: Vec<(String, u64)> = self
                .data
                .views
                .iter()
                .map(|(k, v)| (k.clone(), v.saved_at))
                .collect();
            by_age.sort_by_key(|(_, t)| *t);
            for (k, _) in by_age.into_iter().take(self.data.views.len() - MAX_VIEWS) {
                self.data.views.remove(&k);
            }
        }
        self.persist();
    }

    /// Writes the file atomically (temporary file, then rename). Failures are logged and
    /// otherwise ignored: losing a remembered view is not worth interrupting the user.
    fn persist(&self) {
        let Some(file) = &self.file else {
            return;
        };
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = file.parent() {
                fs::create_dir_all(dir)?;
            }
            let temp = file.with_extension("json.tmp");
            fs::write(&temp, serde_json::to_vec_pretty(&self.data)?)?;
            fs::rename(&temp, file)
        })();
        if let Err(e) = result {
            crate::applog::warn(format!("could not save app state: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ZoomMode;

    fn temp_store(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-output/store");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join(name);
        let _ = fs::remove_file(&file);
        file
    }

    fn view(page: u32) -> ViewState {
        ViewState {
            page,
            offset: 0.25,
            zoom: 1.5,
            zoom_mode: ZoomMode::Custom,
            rotation: 90,
        }
    }

    #[test]
    fn recent_files_are_unique_newest_first_and_capped() {
        let file = temp_store("recent.json");
        let mut store = Store::load(Some(file.clone()));
        for i in 0..25 {
            store.add_recent(Path::new(&format!("C:/docs/{i}.pdf")), |a, b| a == b);
        }
        store.add_recent(Path::new("C:/docs/10.pdf"), |a, b| a == b);
        assert_eq!(store.recent().len(), MAX_RECENT);
        assert_eq!(store.recent()[0].path, Path::new("C:/docs/10.pdf"));
        assert_eq!(
            store
                .recent()
                .iter()
                .filter(|e| e.path == Path::new("C:/docs/10.pdf"))
                .count(),
            1
        );
        // Survives a restart.
        let reloaded = Store::load(Some(file));
        assert_eq!(reloaded.recent(), store.recent());
    }

    #[test]
    fn views_round_trip() {
        let file = temp_store("views.json");
        let mut store = Store::load(Some(file.clone()));
        store.set_view("c:/docs/a.pdf".into(), view(41));
        let reloaded = Store::load(Some(file));
        assert_eq!(reloaded.view("c:/docs/a.pdf"), Some(view(41)));
        assert_eq!(reloaded.view("c:/docs/b.pdf"), None);
    }

    #[test]
    fn settings_round_trip_and_default_for_old_files() {
        let file = temp_store("settings.json");
        let mut store = Store::load(Some(file.clone()));
        assert_eq!(*store.settings(), StoredSettings::default());
        store.set_settings(StoredSettings {
            author: Some("Ada Lovelace".into()),
            appearance: Appearance::Dark,
        });
        let reloaded = Store::load(Some(file.clone()));
        assert_eq!(reloaded.settings().author.as_deref(), Some("Ada Lovelace"));
        assert_eq!(reloaded.settings().appearance, Appearance::Dark);
        // A state file from before Settings existed still loads, with defaults.
        fs::write(&file, br#"{"recent":[],"views":{}}"#).unwrap();
        assert_eq!(
            *Store::load(Some(file)).settings(),
            StoredSettings::default()
        );
    }

    #[test]
    fn unreadable_file_starts_empty() {
        let file = temp_store("broken.json");
        fs::write(&file, b"{ not json").unwrap();
        let store = Store::load(Some(file));
        assert!(store.recent().is_empty());
    }
}
