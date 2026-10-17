//! The system's menu bar on macOS (section 8, ADR 0021). The frontend lays its menus out
//! (`src/lib/menus.svelte.ts`) and sends them here whenever they change: each window its
//! own, since each shows its own documents. The menu bar is the app's, so it shows the menus
//! of the window in use, and a choice in it goes to that window as the item's id
//! (`menu-command`), which the frontend runs. Elsewhere the menus are in the title bar and
//! nothing is sent.
//!
//! The menus are built off the main thread (Tauri's menu calls run there and wait for it),
//! one window's at a time. A change that keeps their shape (labels, enabled and checked
//! states, shortcuts) is made to the items in place, so a menu that is open stays open.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::ipc::{MenuEntry, SystemMenuItem};

/// Event carrying the id of the item chosen.
const MENU_COMMAND: &str = "menu-command";
/// Before the frontend's ids, so the system's own items (which have ids of their own) are
/// told from them.
const ID_PREFIX: &str = "lectrix:";

#[derive(Default)]
pub struct Menus {
    /// Each window's menus, as it last sent them.
    by_window: Mutex<HashMap<String, Vec<MenuEntry>>>,
    /// The menus in the menu bar. Locked while they are built or changed, which waits for
    /// the main thread: never taken there.
    shown: Mutex<Option<Shown>>,
    /// The window whose menus are shown, which gets the choices made in them. Locked only
    /// for a moment, so the main thread can take it.
    target: Mutex<Option<String>>,
}

struct Shown {
    label: String,
    model: Vec<MenuEntry>,
    /// The items by the frontend's id, and the submenus in the order `model` has them.
    items: HashMap<String, MenuItemKind<Wry>>,
    submenus: Vec<Submenu<Wry>>,
}

/// A window sent its menus. They are shown if it is the window in use, or no menus are
/// shown yet.
pub fn set(app: &AppHandle, label: &str, menus: Vec<MenuEntry>) {
    let state = app.state::<Menus>();
    if let Ok(mut by_window) = state.by_window.lock() {
        by_window.insert(label.to_owned(), menus);
    }
    let in_use = app
        .state::<crate::AppState>()
        .windows
        .focused()
        .is_none_or(|focused| focused == label);
    let nothing_shown = state.target.lock().map(|t| t.is_none()).unwrap_or(false);
    if in_use || nothing_shown {
        show(app, label);
    }
}

/// Another window is in use: its menus take the menu bar. Called on the main thread, so the
/// menus are built elsewhere.
pub fn focused(app: &AppHandle, label: &str) {
    let app = app.clone();
    let label = label.to_owned();
    tauri::async_runtime::spawn_blocking(move || show(&app, &label));
}

/// A window closed: its menus go with it.
pub fn forget(app: &AppHandle, label: &str) {
    let state = app.state::<Menus>();
    if let Ok(mut by_window) = state.by_window.lock() {
        by_window.remove(label);
    }
}

/// An item was chosen: its id goes to the window whose menus are shown.
pub fn chosen(app: &AppHandle, event: &MenuEvent) {
    let Some(id) = event.id().as_ref().strip_prefix(ID_PREFIX) else {
        return;
    };
    let target = app
        .state::<Menus>()
        .target
        .lock()
        .ok()
        .and_then(|t| t.clone());
    if let Some(label) = target
        && let Err(e) = app.emit_to(label.as_str(), MENU_COMMAND, id)
    {
        crate::applog::warn(format!("could not pass on the menu item {id}: {e}"));
    }
}

/// Shows the menus of window `label`, changing those shown in place where they keep their
/// shape.
fn show(app: &AppHandle, label: &str) {
    let state = app.state::<Menus>();
    let Some(model) = state
        .by_window
        .lock()
        .ok()
        .and_then(|m| m.get(label).cloned())
    else {
        return;
    };
    let Ok(mut shown) = state.shown.lock() else {
        return;
    };
    let result = match shown.as_mut() {
        Some(current) if current.label == label && current.model == model => Ok(()),
        Some(current) if same_shape(&current.model, &model) => {
            let changed = update(current, &model);
            current.label = label.to_owned();
            current.model = model;
            changed
        }
        _ => build(app, label, model).map(|built| {
            *shown = Some(built);
        }),
    };
    match result {
        Ok(()) => {
            if let Ok(mut target) = state.target.lock() {
                *target = Some(label.to_owned());
            }
        }
        Err(e) => crate::applog::warn(format!("could not show the menus: {e}")),
    }
}

/// Whether two menus have the same items in the same places (what each says, and whether it
/// is enabled or checked, aside).
fn same_shape(a: &[MenuEntry], b: &[MenuEntry]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|pair| match pair {
            (
                MenuEntry::Item {
                    id: a, checked: ca, ..
                },
                MenuEntry::Item {
                    id: b, checked: cb, ..
                },
            ) => a == b && ca.is_some() == cb.is_some(),
            (MenuEntry::Separator, MenuEntry::Separator) => true,
            (MenuEntry::Submenu { items: a, .. }, MenuEntry::Submenu { items: b, .. }) => {
                same_shape(a, b)
            }
            (MenuEntry::System { item: a }, MenuEntry::System { item: b }) => a == b,
            _ => false,
        })
}

/// Makes the menus shown say what `model` says. They have its shape (`same_shape`).
fn update(shown: &Shown, model: &[MenuEntry]) -> tauri::Result<()> {
    let mut submenus = shown.submenus.iter();
    update_entries(&shown.model, model, &shown.items, &mut submenus)
}

fn update_entries<'a>(
    old: &[MenuEntry],
    new: &[MenuEntry],
    items: &HashMap<String, MenuItemKind<Wry>>,
    submenus: &mut impl Iterator<Item = &'a Submenu<Wry>>,
) -> tauri::Result<()> {
    for (old, new) in old.iter().zip(new) {
        match (old, new) {
            (
                MenuEntry::Item {
                    label: old_label,
                    enabled: old_enabled,
                    checked: old_checked,
                    accelerator: old_accelerator,
                    ..
                },
                MenuEntry::Item {
                    id,
                    label,
                    enabled,
                    checked,
                    accelerator,
                },
            ) => match items.get(id) {
                Some(MenuItemKind::MenuItem(item)) => {
                    if label != old_label {
                        item.set_text(label)?;
                    }
                    if enabled != old_enabled {
                        item.set_enabled(*enabled)?;
                    }
                    if accelerator != old_accelerator {
                        item.set_accelerator(accelerator.as_deref())?;
                    }
                }
                Some(MenuItemKind::Check(item)) => {
                    if label != old_label {
                        item.set_text(label)?;
                    }
                    if enabled != old_enabled {
                        item.set_enabled(*enabled)?;
                    }
                    if accelerator != old_accelerator {
                        item.set_accelerator(accelerator.as_deref())?;
                    }
                    if checked != old_checked {
                        item.set_checked(checked.unwrap_or(false))?;
                    }
                }
                _ => {}
            },
            (
                MenuEntry::Submenu {
                    label: old_label,
                    enabled: old_enabled,
                    items: old_items,
                },
                MenuEntry::Submenu {
                    label,
                    enabled,
                    items: new_items,
                },
            ) => {
                if let Some(submenu) = submenus.next() {
                    if label != old_label {
                        submenu.set_text(label)?;
                    }
                    if enabled != old_enabled {
                        submenu.set_enabled(*enabled)?;
                    }
                }
                update_entries(old_items, new_items, items, submenus)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Builds the menus of `model` and puts them in the menu bar (macOS only).
fn build(app: &AppHandle, label: &str, model: Vec<MenuEntry>) -> tauri::Result<Shown> {
    let menu = Menu::new(app)?;
    let mut items = HashMap::new();
    let mut submenus = Vec::new();
    for entry in &model {
        // At the top level, only menus: the system's menu bar has nothing else.
        if let MenuEntry::Submenu {
            label,
            enabled,
            items: children,
        } = entry
        {
            let submenu = build_submenu(app, label, *enabled, children, &mut items, &mut submenus)?;
            menu.append(&submenu)?;
        }
    }
    #[cfg(target_os = "macos")]
    app.set_menu(menu)?;
    #[cfg(not(target_os = "macos"))]
    drop(menu);
    Ok(Shown {
        label: label.to_owned(),
        model,
        items,
        submenus,
    })
}

fn build_submenu(
    app: &AppHandle,
    label: &str,
    enabled: bool,
    entries: &[MenuEntry],
    items: &mut HashMap<String, MenuItemKind<Wry>>,
    submenus: &mut Vec<Submenu<Wry>>,
) -> tauri::Result<Submenu<Wry>> {
    let submenu = Submenu::new(app, label, enabled)?;
    // Before its own submenus, in the order `update_entries` walks them.
    submenus.push(submenu.clone());
    for entry in entries {
        match entry {
            MenuEntry::Item {
                id,
                label,
                enabled,
                checked,
                accelerator,
            } => {
                let full_id = format!("{ID_PREFIX}{id}");
                let accelerator = accelerator.as_deref();
                let kind = match checked {
                    Some(checked) => MenuItemKind::Check(CheckMenuItem::with_id(
                        app,
                        full_id,
                        label,
                        *enabled,
                        *checked,
                        accelerator,
                    )?),
                    None => MenuItemKind::MenuItem(MenuItem::with_id(
                        app,
                        full_id,
                        label,
                        *enabled,
                        accelerator,
                    )?),
                };
                append_kind(&submenu, &kind)?;
                items.insert(id.clone(), kind);
            }
            MenuEntry::Separator => submenu.append(&PredefinedMenuItem::separator(app)?)?,
            MenuEntry::Submenu {
                label,
                enabled,
                items: children,
            } => {
                let child = build_submenu(app, label, *enabled, children, items, submenus)?;
                submenu.append(&child)?;
            }
            MenuEntry::System { item } => submenu.append(&system_item(app, *item)?)?,
        }
    }
    Ok(submenu)
}

fn append_kind(submenu: &Submenu<Wry>, kind: &MenuItemKind<Wry>) -> tauri::Result<()> {
    let item: &dyn IsMenuItem<Wry> = match kind {
        MenuItemKind::MenuItem(item) => item,
        MenuItemKind::Check(item) => item,
        MenuItemKind::Submenu(item) => item,
        MenuItemKind::Predefined(item) => item,
        MenuItemKind::Icon(item) => item,
    };
    submenu.append(item)
}

fn system_item(app: &AppHandle, item: SystemMenuItem) -> tauri::Result<PredefinedMenuItem<Wry>> {
    match item {
        SystemMenuItem::Cut => PredefinedMenuItem::cut(app, None),
        SystemMenuItem::Copy => PredefinedMenuItem::copy(app, None),
        SystemMenuItem::Paste => PredefinedMenuItem::paste(app, None),
        SystemMenuItem::SelectAll => PredefinedMenuItem::select_all(app, None),
        SystemMenuItem::Services => PredefinedMenuItem::services(app, None),
        SystemMenuItem::Hide => PredefinedMenuItem::hide(app, None),
        SystemMenuItem::HideOthers => PredefinedMenuItem::hide_others(app, None),
        SystemMenuItem::ShowAll => PredefinedMenuItem::show_all(app, None),
        SystemMenuItem::Minimize => PredefinedMenuItem::minimize(app, None),
        SystemMenuItem::Zoom => PredefinedMenuItem::maximize(app, None),
        SystemMenuItem::BringAllToFront => PredefinedMenuItem::bring_all_to_front(app, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, label: &str, checked: Option<bool>) -> MenuEntry {
        MenuEntry::Item {
            id: id.to_owned(),
            label: label.to_owned(),
            enabled: true,
            checked,
            accelerator: None,
        }
    }

    fn menu(label: &str, items: Vec<MenuEntry>) -> MenuEntry {
        MenuEntry::Submenu {
            label: label.to_owned(),
            enabled: true,
            items,
        }
    }

    #[test]
    fn shape_ignores_what_items_say_but_not_where_they_are() {
        let a = vec![menu(
            "Edit",
            vec![item("undo", "Undo", None), MenuEntry::Separator],
        )];
        let relabelled = vec![menu(
            "Edit",
            vec![
                item("undo", "Undo add bookmark", None),
                MenuEntry::Separator,
            ],
        )];
        assert!(same_shape(&a, &relabelled));
        let moved = vec![menu(
            "Edit",
            vec![MenuEntry::Separator, item("undo", "Undo", None)],
        )];
        assert!(!same_shape(&a, &moved));
        let checkable = vec![menu(
            "Edit",
            vec![item("undo", "Undo", Some(false)), MenuEntry::Separator],
        )];
        assert!(!same_shape(&a, &checkable));
        let more = vec![menu(
            "Edit",
            vec![
                item("undo", "Undo", None),
                MenuEntry::Separator,
                item("redo", "Redo", None),
            ],
        )];
        assert!(!same_shape(&a, &more));
    }

    #[test]
    fn menus_arrive_as_the_frontend_writes_them() {
        let json = r#"[{"kind":"submenu","label":"Edit","enabled":true,"items":[
            {"kind":"item","id":"undo","label":"Undo","enabled":false,"checked":null,"accelerator":"Cmd+Z"},
            {"kind":"separator"},
            {"kind":"system","item":"selectAll"}]}]"#;
        let menus: Vec<MenuEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(
            menus,
            vec![MenuEntry::Submenu {
                label: "Edit".to_owned(),
                enabled: true,
                items: vec![
                    MenuEntry::Item {
                        id: "undo".to_owned(),
                        label: "Undo".to_owned(),
                        enabled: false,
                        checked: None,
                        accelerator: Some("Cmd+Z".to_owned()),
                    },
                    MenuEntry::Separator,
                    MenuEntry::System {
                        item: SystemMenuItem::SelectAll
                    },
                ],
            }]
        );
    }
}
