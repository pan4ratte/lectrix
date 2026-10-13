//! Remembered app state: recent files and the view position of each file (AGENTS.md
//! section 6.1), settings, the panes and the window's place (placement.rs). Stored as JSON
//! in the app's local data folder, never in the PDF.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ipc::{
    Appearance, PageAdjustments, PaneLayout, QuickTool, ScrollMode, ToolbarDock, ToolbarPosition,
    ToolbarStyle, ToolbarVisibility, ViewState,
};
use crate::placement::WindowPlacement;

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
    /// When a check for updates last got an answer from GitHub, in seconds since the Unix
    /// epoch: the automatic check waits a day after it (ADR 0011).
    #[serde(default)]
    last_update_check: Option<u64>,
    /// Where the main window was and whether it was maximized (placement.rs).
    #[serde(default)]
    window: Option<WindowPlacement>,
    /// The version of Lectrix that last started, to tell the first start after an update
    /// (What's new, section 6.6). Not kept before 1.1.0.
    #[serde(default)]
    last_version: Option<String>,
}

/// The pane layout, reading files from before panels could move, which named the left
/// pane the sidebar and the right one the annotation pane, and files that name a panel
/// this version does not have (the Page labels panel of versions up to 1.1), which is
/// left out. A layout that cannot be read gives the default.
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
        let known = |panel: &serde_json::Value| {
            serde_json::from_value::<crate::ipc::PanelId>(panel.clone()).is_ok()
        };
        for panels in ["leftPanels", "rightPanels"] {
            if let Some(serde_json::Value::Array(list)) = map.get_mut(panels) {
                list.retain(known);
            }
        }
        for active in ["leftActive", "rightActive"] {
            if map.get(active).is_some_and(|p| !p.is_null() && !known(p)) {
                map.remove(active);
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
    /// How pages are laid out when a document opens.
    #[serde(default)]
    pub scroll_mode: ScrollMode,
    #[serde(default)]
    pub toolbar_style: ToolbarStyle,
    #[serde(default)]
    pub toolbar_dock: ToolbarDock,
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
    #[serde(default = "yes")]
    pub smooth_page_turn: bool,
    #[serde(default = "yes")]
    pub smooth_bookmark_scroll: bool,
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
    /// Whether selected text shows a handle at its start and at its end.
    #[serde(default)]
    pub selection_handles: bool,
    /// Whether a file that claims PDF/A opens for reading only until editing is enabled.
    #[serde(default = "yes")]
    pub pdfa_read_only: bool,
    /// Whether a link on a page is tinted while the pointer is on it.
    #[serde(default = "yes")]
    pub highlight_links: bool,
    /// Whether a tooltip on the link under the pointer says where it leads.
    #[serde(default = "yes")]
    pub link_previews: bool,
    /// The visual adjustments every document is shown with, unless it has its own.
    #[serde(default)]
    pub page_adjustments: PageAdjustments,
    /// Whether a colour button opens its panel when the pointer rests on it, without a
    /// click.
    #[serde(default = "yes")]
    pub color_panel_on_hover: bool,
    /// The colour panel's presets that differ from the defaults (section 6.5): a preset's
    /// place in the panel, from 0, and its colour ("#rrggbb").
    #[serde(default)]
    pub preset_colors: BTreeMap<u8, String>,
    /// Keyboard shortcuts that differ from the defaults (section 6.6): a command's name
    /// and its keys, as Settings shows them ("Ctrl+Shift+S"); no keys, no shortcut.
    #[serde(default)]
    pub shortcuts: BTreeMap<String, Vec<String>>,
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

/// The colour panel has this many presets (`PRESET_COLORS` in the frontend's `tools.ts`).
pub const PRESET_COLOR_COUNT: u8 = 18;

/// `value` as a preset's colour is stored, "#rrggbb" in lower case; `None` if it is not
/// six hex digits after a `#`.
pub fn preset_color(value: &str) -> Option<String> {
    let digits = value.strip_prefix('#')?;
    (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("#{}", digits.to_ascii_lowercase()))
}

impl Default for StoredSettings {
    fn default() -> Self {
        StoredSettings {
            author: None,
            appearance: Appearance::default(),
            scroll_mode: ScrollMode::default(),
            toolbar_style: ToolbarStyle::default(),
            toolbar_dock: ToolbarDock::default(),
            toolbar_position: ToolbarPosition::default(),
            toolbar_visibility: ToolbarVisibility::default(),
            quick_tools: default_quick_tools(),
            check_for_updates: true,
            smooth_zoom: true,
            smooth_annotation_scroll: true,
            smooth_page_turn: true,
            smooth_bookmark_scroll: true,
            tooltip_delay_ms: DEFAULT_TOOLTIP_DELAY_MS,
            open_comment_after_markup: false,
            remember_annotation_style: true,
            selection_handles: false,
            pdfa_read_only: true,
            highlight_links: true,
            link_previews: true,
            page_adjustments: PageAdjustments::default(),
            color_panel_on_hover: true,
            preset_colors: BTreeMap::new(),
            shortcuts: BTreeMap::new(),
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
    /// The file was there and read: Lectrix has started before.
    existed: bool,
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Store {
    /// Loads `file`, or starts empty if it does not exist or cannot be read.
    pub fn load(file: Option<PathBuf>) -> Store {
        let data: Option<Data> = file
            .as_deref()
            .and_then(|f| fs::read(f).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        Store {
            file,
            existed: data.is_some(),
            data: data.unwrap_or_default(),
        }
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

    pub fn last_update_check(&self) -> Option<u64> {
        self.data.last_update_check
    }

    pub fn set_last_update_check(&mut self, at: u64) {
        self.data.last_update_check = Some(at);
        self.persist();
    }

    /// Notes that `current` is the version running. True if it is newer than the one that
    /// started last: Lectrix was updated. A state file without a version is from before
    /// versions were kept (1.0.0), so that is an update too; a first start is not, and
    /// neither is going back to an older version.
    pub fn note_version(&mut self, current: &semver::Version) -> bool {
        let now = current.to_string();
        let previous = self.data.last_version.replace(now.clone());
        if previous.as_deref() != Some(now.as_str()) {
            self.persist();
        }
        match previous {
            None => self.existed,
            Some(previous) => {
                semver::Version::parse(&previous).map_or(previous != now, |p| p < *current)
            }
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

    pub fn window_placement(&self) -> Option<WindowPlacement> {
        self.data.window
    }

    /// Notes the window's place in memory only: the window moves many times a second while
    /// dragged. It is written with the next change of anything else, or by `save`.
    pub fn set_window_placement(&mut self, placement: WindowPlacement) {
        self.data.window = Some(placement);
    }

    /// Writes everything now (when Lectrix quits, for the window's place).
    pub fn save(&self) {
        self.persist();
    }

    pub fn view(&self, key: &str) -> Option<ViewState> {
        // (Normalized on the way out too: a view stored up to 1.1 has its adjustments
        // under their old name.)
        self.data
            .views
            .get(key)
            .map(|v| v.view.clone().normalized())
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

    #[test]
    fn preset_colours_are_six_hex_digits() {
        assert_eq!(preset_color("#12AB34").as_deref(), Some("#12ab34"));
        assert_eq!(preset_color("#000000").as_deref(), Some("#000000"));
        for bad in [
            "", "#", "12ab34", "#12ab3", "#12ab345", "#12ab3g", "red", "#fff", "# 2ab34",
        ] {
            assert_eq!(preset_color(bad), None, "{bad}");
        }
    }
    use crate::ipc::{PageAdjustments, PanelId, ZoomMode};

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
            own_adjustments: Some(PageAdjustments {
                brightness: 85,
                contrast: 120,
                invert: true,
                grayscale: false,
                sepia: 40,
            }),
            adjustments: None,
            page_turns: BTreeMap::from([(2, 90), (40, 270)]),
        }
    }

    #[test]
    fn a_view_stored_before_pages_were_turned_has_none_turned() {
        let file = temp_store("view-without-turns.json");
        fs::write(
            &file,
            r#"{"views":{"c:/docs/a.pdf":{"view":{"page":3,"offset":0.5,"zoom":1.25,"zoomMode":"custom","rotation":90,"adjustments":{"invert":true}},"saved_at":1}}}"#,
        )
        .unwrap();
        let stored = Store::load(Some(file))
            .view("c:/docs/a.pdf")
            .expect("the view was read");
        assert_eq!(stored.rotation, 90);
        assert!(stored.page_turns.is_empty());
    }

    #[test]
    fn only_quarter_turns_are_kept() {
        let wild = ViewState {
            page_turns: BTreeMap::from([(0, 90), (1, 0), (2, 45), (3, 360), (4, 180), (5, 270)]),
            ..view(0)
        };
        assert_eq!(
            wild.normalized().page_turns,
            BTreeMap::from([(0, 90), (4, 180), (5, 270)])
        );
        assert_eq!(view(7).normalized(), view(7));
    }

    #[test]
    fn a_view_stored_before_adjustments_has_none() {
        let file = temp_store("view-without-adjustments.json");
        fs::write(
            &file,
            r#"{"views":{"c:/docs/a.pdf":{"view":{"page":3,"offset":0.5,"zoom":1.25,"zoomMode":"custom","rotation":0},"saved_at":1}}}"#,
        )
        .unwrap();
        let store = Store::load(Some(file.clone()));
        let stored = store.view("c:/docs/a.pdf").expect("the view was read");
        assert_eq!(stored.page, 3);
        assert_eq!(stored.own_adjustments, None);
        // Up to 1.1 a view had its adjustments as `adjustments`, whether anything was
        // adjusted or not. What was adjusted is the document's own now; a view with the
        // pages as they are follows the adjustments Settings give every document.
        fs::write(
            &file,
            r#"{"views":{
                "c:/docs/dark.pdf":{"view":{"page":0,"offset":0,"zoom":1,"zoomMode":"custom","rotation":0,"adjustments":{"brightness":90,"contrast":100,"invert":true,"grayscale":false}},"saved_at":1},
                "c:/docs/plain.pdf":{"view":{"page":0,"offset":0,"zoom":1,"zoomMode":"custom","rotation":0,"adjustments":{"brightness":100,"contrast":100,"invert":false,"grayscale":false}},"saved_at":1},
                "c:/docs/new.pdf":{"view":{"page":0,"offset":0,"zoom":1,"zoomMode":"custom","rotation":0,"ownAdjustments":{"sepia":30}},"saved_at":1}
            }}"#,
        )
        .unwrap();
        let store = Store::load(Some(file.clone()));
        let dark = store.view("c:/docs/dark.pdf").expect("the view was read");
        assert_eq!(
            dark.own_adjustments,
            Some(PageAdjustments {
                brightness: 90,
                invert: true,
                ..PageAdjustments::default()
            })
        );
        assert_eq!(dark.adjustments, None);
        assert_eq!(
            store
                .view("c:/docs/plain.pdf")
                .and_then(|v| v.own_adjustments),
            None
        );
        assert_eq!(
            store
                .view("c:/docs/new.pdf")
                .and_then(|v| v.own_adjustments),
            Some(PageAdjustments {
                sepia: 30,
                ..PageAdjustments::default()
            })
        );
        // The old name is not written again.
        let mut store = store;
        store.set_view(
            "c:/docs/dark.pdf".to_string(),
            ViewState { page: 1, ..dark },
        );
        let written = fs::read_to_string(&file).unwrap();
        let dark: serde_json::Value = serde_json::from_str::<serde_json::Value>(&written).unwrap()
            ["views"]["c:/docs/dark.pdf"]["view"]
            .clone();
        assert!(dark.get("adjustments").is_none(), "{dark}");
        assert_eq!(dark["ownAdjustments"]["invert"], true);
        // One with some of them (sepia came later; a later version may add more) keeps what
        // it has.
        let partial: PageAdjustments = serde_json::from_str(r#"{"invert":true}"#).unwrap();
        assert_eq!(
            partial,
            PageAdjustments {
                invert: true,
                ..PageAdjustments::default()
            }
        );
    }

    #[test]
    fn adjustments_stay_within_their_range() {
        let wild = PageAdjustments {
            brightness: 0,
            contrast: 900,
            invert: true,
            grayscale: true,
            sepia: 250,
        };
        assert_eq!(
            wild.clamped(),
            PageAdjustments {
                brightness: PageAdjustments::MIN,
                contrast: PageAdjustments::MAX,
                invert: true,
                grayscale: true,
                sepia: PageAdjustments::SEPIA_MAX,
            }
        );
        assert_eq!(
            PageAdjustments::default().clamped(),
            PageAdjustments::default()
        );
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
            scroll_mode: ScrollMode::TwoPage,
            toolbar_style: ToolbarStyle::Floating,
            toolbar_dock: ToolbarDock::Right,
            toolbar_position: ToolbarPosition::Left,
            toolbar_visibility: ToolbarVisibility::OnHover,
            quick_tools: vec![QuickTool::Squiggly, QuickTool::Bookmark],
            check_for_updates: false,
            smooth_zoom: false,
            smooth_annotation_scroll: false,
            smooth_page_turn: false,
            smooth_bookmark_scroll: false,
            tooltip_delay_ms: 800,
            open_comment_after_markup: true,
            remember_annotation_style: false,
            selection_handles: true,
            pdfa_read_only: false,
            highlight_links: false,
            link_previews: false,
            page_adjustments: PageAdjustments {
                sepia: 45,
                contrast: 110,
                ..PageAdjustments::default()
            },
            color_panel_on_hover: false,
            preset_colors: BTreeMap::from([
                (2, "#12ab34".to_string()),
                (17, "#101010".to_string()),
            ]),
            shortcuts: BTreeMap::from([
                ("save".to_string(), vec!["Ctrl+Alt+S".to_string()]),
                ("copy".to_string(), Vec::new()),
            ]),
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
        // So does one from v1, before the toolbar settings: the toolbar is docked above the
        // pages, as in a new install.
        fs::write(
            &file,
            br#"{"settings":{"author":"Ada","appearance":"light"}}"#,
        )
        .unwrap();
        let v1 = Store::load(Some(file.clone()));
        assert_eq!(v1.settings().author.as_deref(), Some("Ada"));
        assert_eq!(v1.settings().quick_tools, QuickTool::DEFAULT.to_vec());
        assert_eq!(v1.settings().toolbar_style, ToolbarStyle::Panel);
        assert_eq!(v1.settings().toolbar_dock, ToolbarDock::Top);
        assert_eq!(v1.settings().toolbar_position, ToolbarPosition::Bottom);
        assert_eq!(v1.settings().scroll_mode, ScrollMode::SinglePageContinuous);
        assert!(v1.settings().check_for_updates);
        assert!(v1.settings().smooth_zoom);
        assert!(v1.settings().smooth_annotation_scroll);
        assert!(v1.settings().smooth_page_turn);
        assert!(v1.settings().smooth_bookmark_scroll);
        assert_eq!(v1.settings().tooltip_delay_ms, DEFAULT_TOOLTIP_DELAY_MS);
        assert!(!v1.settings().open_comment_after_markup);
        assert!(v1.settings().remember_annotation_style);
        assert!(!v1.settings().selection_handles);
        assert!(v1.settings().pdfa_read_only);
        assert!(v1.settings().highlight_links);
        assert!(v1.settings().link_previews);
        assert!(v1.settings().color_panel_on_hover);
        assert!(v1.settings().preset_colors.is_empty());
        assert!(v1.settings().shortcuts.is_empty());
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
            right_panels: vec![PanelId::Bookmarks],
            left_active: Some(PanelId::Pages),
            right_active: None,
            thumbnail_width: 184,
            thumbnails_fit: true,
            thumbnails_grid: true,
            recent_files_grid: true,
            combine_width: 300,
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
        assert_eq!(partial.thumbnail_width, 112);
        assert!(!partial.thumbnails_fit);
        assert!(!partial.thumbnails_grid);
        assert_eq!(partial.combine_width, 224);
        // From before panels could move: the sidebar and annotation pane's names.
        fs::write(
            &file,
            br#"{"panes":{"sidebarOpen":false,"sidebarWidth":200,"annotationsOpen":true,"annotationsWidth":350}}"#,
        )
        .unwrap();
        let old = Store::load(Some(file.clone())).panes();
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
        // From when page labels had a panel: it is left out, and the rest stays.
        fs::write(
            &file,
            br#"{"panes":{"leftWidth":310,"leftPanels":["pages","labels"],"rightPanels":["labels","annotations","bookmarks"],"leftActive":"labels","rightActive":"bookmarks"}}"#,
        )
        .unwrap();
        let before = Store::load(Some(file)).panes();
        assert_eq!(before.left_width, 310);
        assert_eq!(before.left_panels, vec![PanelId::Pages]);
        assert_eq!(
            before.right_panels,
            vec![PanelId::Annotations, PanelId::Bookmarks]
        );
        assert_eq!(before.left_active, None);
        assert_eq!(before.right_active, Some(PanelId::Bookmarks));
    }

    #[test]
    fn window_placement_is_written_by_save_or_the_next_change() {
        use crate::placement::Bounds;
        let file = temp_store("window.json");
        let mut store = Store::load(Some(file.clone()));
        assert_eq!(store.window_placement(), None);
        let placement = WindowPlacement {
            bounds: Some(Bounds {
                x: -8,
                y: 40,
                width: 1300,
                height: 900,
            }),
            maximized: true,
        };
        store.set_window_placement(placement);
        // In memory only, until saved.
        assert_eq!(Store::load(Some(file.clone())).window_placement(), None);
        store.save();
        assert_eq!(
            Store::load(Some(file.clone())).window_placement(),
            Some(placement)
        );
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
    fn last_update_check_round_trips() {
        let file = temp_store("last-check.json");
        let mut store = Store::load(Some(file.clone()));
        assert_eq!(store.last_update_check(), None);
        store.set_last_update_check(1_791_000_000);
        assert_eq!(
            Store::load(Some(file)).last_update_check(),
            Some(1_791_000_000)
        );
    }

    #[test]
    fn an_update_is_told_once_and_a_first_start_or_downgrade_never() {
        let version = |v: &str| semver::Version::parse(v).unwrap();
        let file = temp_store("version.json");
        // A first start: no state file yet.
        let mut store = Store::load(Some(file.clone()));
        assert!(!store.note_version(&version("1.1.0")));
        assert!(!Store::load(Some(file.clone())).note_version(&version("1.1.0")));
        // Updated, then the next start.
        assert!(Store::load(Some(file.clone())).note_version(&version("1.2.0-beta.1")));
        assert!(!Store::load(Some(file.clone())).note_version(&version("1.2.0-beta.1")));
        assert!(Store::load(Some(file.clone())).note_version(&version("1.2.0")));
        // Back to an older version.
        assert!(!Store::load(Some(file.clone())).note_version(&version("1.1.0")));

        // A state file from 1.0.0, which kept no version.
        let old = temp_store("version-1.0.0.json");
        fs::write(&old, r#"{"recent": []}"#).unwrap();
        assert!(Store::load(Some(old.clone())).note_version(&version("1.1.0")));
        assert!(!Store::load(Some(old)).note_version(&version("1.1.0")));
        // Test and measurement runs keep no state file.
        assert!(!Store::load(None).note_version(&version("1.1.0")));
    }

    #[test]
    fn unreadable_file_starts_empty() {
        let file = temp_store("broken.json");
        fs::write(&file, b"{ not json").unwrap();
        let store = Store::load(Some(file));
        assert!(store.recent().is_empty());
    }
}
