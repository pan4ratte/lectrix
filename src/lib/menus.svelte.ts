// The app's menus (section 8), laid out once: the title bar shows them on Windows and Linux
// (MenuBar.svelte), and on macOS they go to the system's menu bar, arranged as Mac apps
// arrange theirs (`NativeMenus`). Read inside a reactive context, the layout follows the
// app's state: what is enabled, checked, and what the items say.

import { commands, setViewMode, type CommandName } from '#lib/commands.ts';
import { APP_NAME } from '#lib/config.ts';
import { debugMenu } from '#lib/features/debug/menu.ts';
import { update } from '#lib/features/update/update.svelte.ts';
import { VIEW_MODES } from '#lib/features/viewer/layout.ts';
import type { MenuEntry as NativeEntry, SystemMenuItem } from '#lib/ipc/index.ts';
import { PLATFORM, type Platform } from '#lib/platform.ts';
import { menuAccelerator, shortcutOf } from '#lib/shortcuts.ts';
import { app } from '#lib/stores/app.svelte.ts';

export type MenuEntry =
	| {
			kind: 'item';
			/** Unique across the menus: how the system's menu bar names the item chosen. */
			id: string;
			label: string;
			run: () => void;
			enabled?: boolean;
			/** A check mark, shown when true (absent: the item has none). */
			checked?: boolean;
			/** The command whose shortcut the item shows. */
			command?: CommandName;
			tip?: string;
			/** Shown in the muted text colour (a recent file that is gone). */
			muted?: boolean;
	  }
	| {
			/** One of several, checked: a radio group in the title bar's menus. */
			kind: 'radio';
			id: string;
			value: string;
			options: readonly { value: string; label: string }[];
			choose: (value: string) => void;
	  }
	| { kind: 'separator' }
	| { kind: 'submenu'; label: string; enabled?: boolean; items: MenuEntry[]; wide?: boolean }
	/** macOS's own item, which the system runs (text editing, Hide, Minimize...). */
	| { kind: 'system'; item: SystemMenuItem };

export interface AppMenu {
	label: string;
	items: MenuEntry[];
}

const separator: MenuEntry = { kind: 'separator' };

function item(id: string, label: string, run: () => void, more: Partial<Extract<MenuEntry, { kind: 'item' }>> = {}): MenuEntry {
	return { kind: 'item', id, label, run, ...more };
}

/** An item that runs a command and shows its shortcut, with the command's name as its id. */
function command(name: CommandName, label: string, enabled = true): MenuEntry {
	return item(name, label, commands[name], { enabled, command: name });
}

function system(item: SystemMenuItem): MenuEntry {
	return { kind: 'system', item };
}

/** A text field has the focus (tracked on macOS, where Undo and Redo act on its text). */
export const editing = $state({ text: false });

/** The menus, as the platform has them. */
export function appMenus(platform: Platform = PLATFORM): AppMenu[] {
	const mac = platform === 'macos';
	const tab = app.active;
	const hasDoc = tab !== null;
	/** A document is being read: its pages are not being organized (section 6.7), where the
	 * viewer and the side panes are not on screen. */
	const reading = tab !== null && !tab.organize;

	const recent: MenuEntry = {
		kind: 'submenu',
		label: 'Open recent',
		enabled: app.recent.length > 0,
		wide: true,
		items: app.recent.map((file) =>
			item(`recent:${file.index}`, file.name, () => void app.openRecent(file.index), {
				tip: `${file.folder}\\${file.name}`,
				muted: !file.exists
			})
		)
	};

	const undoName = app.combineActive ? null : tab?.state.undoName;
	const redoName = app.combineActive ? null : tab?.state.redoName;
	const canUndo = app.combineActive ? !!app.combine?.canUndo : !!undoName;
	const canRedo = app.combineActive ? !!app.combine?.canRedo : !!redoName;
	// On macOS, Undo and Redo also edit a text field that has the focus, since the menu bar
	// gets their keys there; they then name no step of the document.
	const text = mac && editing.text;
	const undo = command('undo', undoName && !text ? `Undo ${undoName.toLowerCase()}` : 'Undo', canUndo || text);
	const redo = command('redo', redoName && !text ? `Redo ${redoName.toLowerCase()}` : 'Redo', canRedo || text);

	const file: AppMenu = {
		label: 'File',
		items: mac
			? [
					command('open', 'Open…'),
					recent,
					command('combineFiles', 'Combine files…'),
					separator,
					command('closeTab', 'Close', hasDoc || app.combineActive),
					command('save', 'Save', hasDoc),
					command('saveAs', 'Save as…', hasDoc),
					command('saveAsOptimized', 'Save as (optimized)…', hasDoc),
					separator,
					command('reload', 'Reload from disk', hasDoc && !tab?.untitled)
				]
			: [
					command('open', 'Open…'),
					recent,
					command('combineFiles', 'Combine files…'),
					separator,
					command('save', 'Save', hasDoc),
					command('saveAs', 'Save as…', hasDoc),
					command('saveAsOptimized', 'Save as (optimized)…', hasDoc),
					separator,
					command('reload', 'Reload from disk', hasDoc && !tab?.untitled),
					command('closeTab', 'Close', hasDoc || app.combineActive),
					command('moveTabToNewWindow', 'Move tab to new window', !!tab && !app.isOnlyTab(tab.id)),
					separator,
					command('settings', 'Settings…'),
					separator,
					command('exit', 'Exit')
				]
	};

	const edit: AppMenu = {
		label: 'Edit',
		items: mac
			? [
					undo,
					redo,
					separator,
					system('cut'),
					system('copy'),
					system('paste'),
					system('selectAll'),
					separator,
					{
						kind: 'submenu',
						label: 'Find',
						enabled: reading,
						items: [
							command('find', 'Find…', reading),
							command('findNext', 'Find next', reading),
							command('findPrevious', 'Find previous', reading)
						]
					}
				]
			: [undo, redo, separator, command('copy', 'Copy', !!tab?.selection), command('find', 'Find…', reading)]
	};

	const view: AppMenu = {
		label: 'View',
		items: [
			item('toggleLeftPane', 'Left pane', commands.toggleLeftPane, {
				enabled: reading && app.panels.left.length > 0,
				checked: reading && app.isOpen('left'),
				command: 'toggleLeftPane'
			}),
			item('toggleRightPane', 'Right pane', commands.toggleRightPane, {
				enabled: reading && app.panels.right.length > 0,
				checked: reading && app.isOpen('right'),
				command: 'toggleRightPane'
			}),
			separator,
			command('showPages', 'Pages', reading),
			command('showBookmarks', 'Bookmarks', reading),
			command('showAnnotations', 'Annotations', reading),
			separator,
			command('zoomIn', 'Zoom in', hasDoc || app.combineActive),
			command('zoomOut', 'Zoom out', hasDoc || app.combineActive),
			command('fitWidth', 'Fit width', reading),
			command('fitPage', 'Fit page', reading),
			separator,
			{
				kind: 'submenu',
				label: 'Page display',
				enabled: reading,
				items: [
					{
						kind: 'radio',
						id: 'viewMode',
						value: tab?.mode ?? '',
						options: VIEW_MODES.map((m) => ({ value: m.id, label: m.label })),
						choose: (value) => {
							const mode = VIEW_MODES.find((m) => m.id === value);
							if (mode) setViewMode(mode.id);
						}
					},
					separator,
					item('toggleCoverPage', 'Cover page alone', commands.toggleCoverPage, {
						enabled: tab?.columns === 2,
						checked: tab?.cover ?? false,
						command: 'toggleCoverPage'
					})
				]
			},
			command('rotateViewClockwise', 'Rotate view clockwise', reading),
			command('rotateViewCounterClockwise', 'Rotate view counter-clockwise', reading),
			command('fullScreen', 'Full screen', reading),
			separator,
			command('goToPage', 'Go to page…', reading),
			command('back', 'Back', reading && !!tab?.history.canGoBack),
			command('forward', 'Forward', reading && !!tab?.history.canGoForward)
		]
	};

	const documentMenu: AppMenu = {
		label: 'Document',
		items: [
			command('organizePages', tab?.organize ? 'Finish organizing pages' : 'Organize pages', hasDoc),
			command('insertPages', 'Insert pages from file…', hasDoc && !!tab?.flags.canAssemble),
			separator,
			command('addBookmark', 'Add bookmark', reading && !!tab?.canEditBookmarks),
			separator,
			command('repairAnnotations', 'Repair annotations…', !!tab?.flags.canAnnotate && tab.allAnnotations.length > 0)
		]
	};

	const checking = update.stage.kind === 'checking';
	const help: AppMenu = {
		label: 'Help',
		items: mac
			? [command('whatsNew', 'What’s new')]
			: [
					command('whatsNew', 'What’s new'),
					command('checkForUpdates', 'Check for updates…', !checking),
					separator,
					command('about', `About ${APP_NAME}`)
				]
	};

	const debug = app.startup?.debugMenu ? [debugMenu()] : [];
	if (!mac) return [file, edit, view, documentMenu, help, ...debug];

	// The application menu, named after the app by the system, and the Window menu.
	const appMenu: AppMenu = {
		label: APP_NAME,
		items: [
			command('about', `About ${APP_NAME}`),
			command('checkForUpdates', 'Check for updates…', !checking),
			separator,
			command('settings', 'Settings…'),
			separator,
			system('services'),
			separator,
			system('hide'),
			system('hideOthers'),
			system('showAll'),
			separator,
			command('exit', `Quit ${APP_NAME}`)
		]
	};
	const windowMenu: AppMenu = {
		label: 'Window',
		items: [
			system('minimize'),
			system('zoom'),
			separator,
			command('previousTab', 'Show previous tab', app.tabs.length > 1),
			command('nextTab', 'Show next tab', app.tabs.length > 1),
			command('moveTabToNewWindow', 'Move tab to new window', !!tab && !app.isOnlyTab(tab.id)),
			separator,
			system('bringAllToFront')
		]
	};
	return [appMenu, file, edit, view, documentMenu, windowMenu, help, ...debug];
}

/** Words a title leaves in lower case, after its first. */
const MINOR_WORDS = new Set(['a', 'an', 'and', 'at', 'by', 'for', 'from', 'in', 'of', 'on', 'or', 'the', 'to', 'with']);

/** A label in title case, as Mac menus write theirs: "Save as (optimized)…" is "Save As
 * (Optimized)…", "Insert pages from file…" "Insert Pages from File…". */
export function titleCase(label: string): string {
	return label
		.split(' ')
		.map((word, i) =>
			i > 0 && MINOR_WORDS.has(word) ? word : word.replace(/(^|-)([^A-Za-z0-9]*)([a-z])/g, (_, dash: string, lead: string, letter: string) => dash + lead + letter.toUpperCase())
		)
		.join(' ');
}

/**
 * The menus as the system's menu bar takes them: ids, labels in title case (not file
 * names, nor the Debug menu's), states and shortcuts, without what the items run. While a
 * dialog is open, nothing in them acts on the window, but Undo and Redo still edit a text
 * field.
 */
export function nativeMenus(menus: readonly AppMenu[], modal: boolean): NativeEntry[] {
	const label = (id: string, text: string) => (id.startsWith('recent:') || id.startsWith('debug:') ? text : titleCase(text));
	const entries = (items: readonly MenuEntry[]): NativeEntry[] =>
		items.flatMap((entry): NativeEntry[] => {
			switch (entry.kind) {
				case 'item': {
					const keys = entry.command ? shortcutOf(entry.command, app.settings?.shortcuts, 'macos') : '';
					const textEdit = editing.text && (entry.id === 'undo' || entry.id === 'redo');
					return [
						{
							kind: 'item',
							id: entry.id,
							label: label(entry.id, entry.label),
							enabled: (entry.enabled ?? true) && (!modal || textEdit),
							checked: entry.checked ?? null,
							accelerator: keys ? menuAccelerator(keys) : null
						}
					];
				}
				case 'radio':
					return entry.options.map((option) => ({
						kind: 'item',
						id: `${entry.id}:${option.value}`,
						label: titleCase(option.label),
						enabled: !modal,
						checked: option.value === entry.value,
						accelerator: null
					}));
				case 'separator':
					return [{ kind: 'separator' }];
				case 'submenu':
					return [{ kind: 'submenu', label: titleCase(entry.label), enabled: (entry.enabled ?? true) && !modal, items: entries(entry.items) }];
				case 'system':
					return [{ kind: 'system', item: entry.item }];
			}
		});
	return menus.map((menu) => ({ kind: 'submenu', label: menu.label, enabled: true, items: entries(menu.items) }));
}

/** Runs the item the system's menu bar names, as the menus now have it; false if none. */
export function runMenuItem(menus: readonly AppMenu[], id: string): boolean {
	for (const entry of menus.flatMap((m) => m.items)) {
		if (find(entry, id)) return true;
	}
	return false;
}

function find(entry: MenuEntry, id: string): boolean {
	switch (entry.kind) {
		case 'item':
			if (entry.id !== id) return false;
			entry.run();
			return true;
		case 'radio': {
			const option = entry.options.find((o) => `${entry.id}:${o.value}` === id);
			if (!option) return false;
			entry.choose(option.value);
			return true;
		}
		case 'submenu':
			return entry.items.some((child) => find(child, id));
		default:
			return false;
	}
}
