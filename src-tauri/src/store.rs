//! Remembered app state: recent files and the view position of each file (AGENTS.md
//! section 6.1). Stored as JSON in the app's local data folder, never in the PDF.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ipc::{
    Appearance, PaneLayout, QuickTool, ToolbarPosition, ToolbarStyle, ToolbarVisibility, ViewState,
};

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
    #[serde(default, deserialize_with = "panes_with_old_names")]
    panes: PaneLayout,
    /// The release the user said not to be asked about again (ADR 0011).
    #[serde(default)]
    skipped_update: Option<String>,
}

/// The pane layout, reading files from before panels could move, which named the left
/// pane the sidebar and the right one the annotation pane. A layout that cannot be read
/// gives the default.
fn panes_with_old_names<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<PaneLayout, D::Error> {
    let mut value = serde_json::Value::deserialize(deserializer)?;
    if let Some(map) = value.as_object_mut() {
        for (old, new) in [
            ("sidebarOpen", "leftOpen"),
            ("sidebarWidth", "leftWidth"),
            ("annotationsOpen", "rightOpen"),
            ("annotationsWidth", "rightWidth"),
        ] {
            if let Some(v) = map.remove(old) {
                map.entry(new).or_insert(v);
            }
        }
    }
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// What the Settings dialog changes (sections 6.5 and 6.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredSettings {
    /// The author name for new annotations; `None` uses the Windows user name.
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub toolbar_style: ToolbarStyle,
    #[serde(default)]
    pub toolbar_position: ToolbarPosition,
    #[serde(default)]
    pub toolbar_visibility: ToolbarVisibility,
    #[serde(
        default = "default_quick_tools",
        deserialize_with = "known_quick_tools"
    )]
    pub quick_tools: Vec<QuickTool>,
    #[serde(default = "yes")]
    pub check_for_updates: bool,
    #[serde(default = "yes")]
    pub smooth_zoom: bool,
    #[serde(default = "yes")]
    pub smooth_annotation_scroll: bool,
    /// How long the pointer rests on an annotation before its comment shows.
    #[serde(default = "default_tooltip_delay")]
    pub tooltip_delay_ms: u32,
    /// Whether marking text up selects the new annotation and opens its comment.
    #[serde(default)]
    pub open_comment_after_markup: bool,
    /// Whether the last colour and opacity given to an annotation of a type is what new
    /// ones of that type get.
    #[serde(default = "yes")]
    pub remember_annotation_style: bool,
}

fn yes() -> bool {
    true
}

/// The comment tooltip's delay until the user sets one (section 6.5).
pub const DEFAULT_TOOLTIP_DELAY_MS: u32 = 300;
/// The longest delay Settings offers.
pub const MAX_TOOLTIP_DELAY_MS: u32 = 2000;

fn default_tooltip_delay() -> u32 {
    DEFAULT_TOOLTIP_DELAY_MS
}

impl Default for StoredSettings {
    fn default() -> Self {
        StoredSettings {
            author: None,
            appearance: Appearance::default(),
            toolbar_style: ToolbarStyle::default(),
            toolbar_position: ToolbarPosition::default(),
            toolbar_visibility: ToolbarVisibility::default(),
            quick_tools: default_quick_tools(),
            check_for_updates: true,
            smooth_zoom: true,
            smooth_annotation_scroll: true,
            tooltip_delay_ms: DEFAULT_TOOLTIP_DELAY_MS,
            open_comment_after_markup: false,
            remember_annotation_style: true,
        }
    }
}

fn default_quick_tools() -> Vec<QuickTool> {
    QuickTool::DEFAULT.to_vec()
}

/// Skips tools this version does not know (written by a newer one), so they cannot make
/// the whole state file unreadable.
fn known_quick_tools<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<QuickTool>, D::Error> {
    let values = Vec::<serde_json::Value>::deserialize(d)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
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

    pub fn skipped_update(&self) -> Option<&str> {
        self.data.skipped_update.as_deref()
    }

    pub fn set_skipped_update(&mut self, version: Option<String>) {
        if self.data.skipped_update != version {
            self.data.skipped_update = version;
            self.persist();
        }
    }

    pub fn panes(&self) -> PaneLayout {
        self.data.panes.clone()
    }

    pub fn set_panes(&mut self, panes: PaneLayout) {
        if self.data.panes != panes {
            self.data.panes = panes;
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
    use crate::ipc::{PanelId, ZoomMode};

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
        let settings = StoredSettings {
            author: Some("Ada Lovelace".into()),
            appearance: Appearance::Dark,
            toolbar_style: ToolbarStyle::Panel,
            toolbar_position: ToolbarPosition::Top,
            toolbar_visibility: ToolbarVisibility::OnHover,
            quick_tools: vec![QuickTool::Squiggly, QuickTool::Bookmark],
            check_for_updates: false,
            smooth_zoom: false,
            smooth_annotation_scroll: false,
            tooltip_delay_ms: 800,
            open_comment_after_markup: true,
            remember_annotation_style: false,
        };
        store.set_settings(settings.clone());
        let reloaded = Store::load(Some(file.clone()));
        assert_eq!(*reloaded.settings(), settings);
        // A state file from before Settings existed still loads, with defaults.
        fs::write(&file, br#"{"recent":[],"views":{}}"#).unwrap();
        assert_eq!(
            *Store::load(Some(file.clone())).settings(),
            StoredSettings::default()
        );
        // So does one from v1, before the toolbar settings.
        fs::write(
            &file,
            br#"{"settings":{"author":"Ada","appearance":"light"}}"#,
        )
        .unwrap();
        let v1 = Store::load(Some(file.clone()));
        assert_eq!(v1.settings().author.as_deref(), Some("Ada"));
        assert_eq!(v1.settings().quick_tools, QuickTool::DEFAULT.to_vec());
        assert_eq!(v1.settings().toolbar_style, ToolbarStyle::Floating);
        assert!(v1.settings().check_for_updates);
        assert!(v1.settings().smooth_zoom);
        assert!(v1.settings().smooth_annotation_scroll);
        assert_eq!(v1.settings().tooltip_delay_ms, DEFAULT_TOOLTIP_DELAY_MS);
        assert!(!v1.settings().open_comment_after_markup);
        assert!(v1.settings().remember_annotation_style);
        // Tools from a newer version are skipped, not fatal; an empty list stays empty.
        fs::write(
            &file,
            br#"{"recent":[{"path":"c:/a.pdf","opened_at":1}],"settings":{"quick_tools":["copy","sparkle"]}}"#,
        )
        .unwrap();
        let newer = Store::load(Some(file.clone()));
        assert_eq!(newer.settings().quick_tools, vec![QuickTool::Copy]);
        assert_eq!(newer.recent().len(), 1);
        fs::write(&file, br#"{"settings":{"quick_tools":[]}}"#).unwrap();
        assert!(Store::load(Some(file)).settings().quick_tools.is_empty());
    }

    #[test]
    fn panes_round_trip_and_default_for_old_files() {
        let file = temp_store("panes.json");
        let mut store = Store::load(Some(file.clone()));
        assert_eq!(store.panes(), PaneLayout::default());
        let panes = PaneLayout {
            left_open: false,
            left_width: 320,
            right_open: true,
            right_width: 400,
            left_panels: vec![PanelId::Annotations, PanelId::Pages],
            right_panels: vec![PanelId::Labels, PanelId::Bookmarks],
            left_active: Some(PanelId::Pages),
            right_active: None,
        };
        store.set_panes(panes.clone());
        assert_eq!(Store::load(Some(file.clone())).panes(), panes);
        // A state file from before the panes were remembered loads with defaults, and one
        // missing a field keeps the others.
        fs::write(&file, br#"{"recent":[],"views":{}}"#).unwrap();
        assert_eq!(
            Store::load(Some(file.clone())).panes(),
            PaneLayout::default()
        );
        fs::write(&file, br#"{"panes":{"leftWidth":300}}"#).unwrap();
        let partial = Store::load(Some(file.clone())).panes();
        assert_eq!(partial.left_width, 300);
        assert!(partial.left_open);
        assert_eq!(partial.right_panels, vec![PanelId::Annotations]);
        // From before panels could move: the sidebar and annotation pane's names.
        fs::write(
            &file,
            br#"{"panes":{"sidebarOpen":false,"sidebarWidth":200,"annotationsOpen":true,"annotationsWidth":350}}"#,
        )
        .unwrap();
        let old = Store::load(Some(file)).panes();
        assert_eq!(
            (
                old.left_open,
                old.left_width,
                old.right_open,
                old.right_width
            ),
            (false, 200, true, 350)
        );
        assert_eq!(old.left_panels, PaneLayout::default().left_panels);
    }

    #[test]
    fn skipped_update_round_trips() {
        let file = temp_store("skipped.json");
        let mut store = Store::load(Some(file.clone()));
        assert_eq!(store.skipped_update(), None);
        store.set_skipped_update(Some("1.2.0".into()));
        assert_eq!(Store::load(Some(file)).skipped_update(), Some("1.2.0"));
    }

    #[test]
    fn unreadable_file_starts_empty() {
        let file = temp_store("broken.json");
        fs::write(&file, b"{ not json").unwrap();
        let store = Store::load(Some(file));
        assert!(store.recent().is_empty());
    }
}
