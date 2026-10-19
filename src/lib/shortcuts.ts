// Keyboard shortcuts (sections 6.6 and 8): the commands that can have one, the keys each
// comes with, and the keys in use once the changes made in Settings are applied. A shortcut
// is written as Settings shows it: "Ctrl+Shift+S", "Alt+Left", "F3". On macOS, Ctrl in a
// shortcut is the Command key (either answers to it), some commands come with the keys Mac
// apps use (`mac`), and shortcuts are shown as the system shows them: "⇧⌘S" (`displayShortcut`).

import type { CommandName } from './commands.ts';
import { PLATFORM, type Platform } from './platform.ts';

export interface ShortcutCommand {
	command: CommandName;
	/** As Settings lists it. */
	label: string;
	/** The shortcuts it comes with. */
	keys: readonly string[];
	/** The shortcuts it comes with on macOS, where they differ: those Mac apps use. */
	mac?: readonly string[];
	/** More shortcuts that work while `keys` are unchanged and are not listed: the same key
	 * elsewhere on a keyboard (the keypad's plus), or a mouse's back button. */
	aliases?: readonly string[];
	/** Also runs while typing in a text field (never from a key that types or edits text). */
	inInputs?: boolean;
}

export interface ShortcutGroup {
	title: string;
	commands: readonly ShortcutCommand[];
}

/** Every command that can have a shortcut, as Settings groups them. */
export const SHORTCUT_GROUPS: readonly ShortcutGroup[] = [
	{
		title: 'File',
		commands: [
			{ command: 'open', label: 'Open', keys: ['Ctrl+O'], inInputs: true },
			{ command: 'combineFiles', label: 'Combine files', keys: [], inInputs: true },
			{ command: 'unlockFile', label: 'Unlock', keys: [], inInputs: true },
			{ command: 'protectFile', label: 'Protect', keys: [], inInputs: true },
			{ command: 'compressFile', label: 'Compress', keys: [], inInputs: true },
			{ command: 'save', label: 'Save', keys: ['Ctrl+S'], inInputs: true },
			{ command: 'saveAs', label: 'Save as', keys: ['Ctrl+Shift+S'], inInputs: true },
			{ command: 'saveAsOptimized', label: 'Save as (optimized)', keys: [], inInputs: true },
			{ command: 'reload', label: 'Reload from disk', keys: [], inInputs: true },
			{ command: 'closeTab', label: 'Close tab', keys: ['Ctrl+W', 'Ctrl+F4'], mac: ['Ctrl+W'], inInputs: true },
			{ command: 'moveTabToNewWindow', label: 'Move tab to new window', keys: [], inInputs: true },
			{ command: 'settings', label: 'Settings', keys: ['Ctrl+,'], inInputs: true },
			{ command: 'exit', label: 'Exit', keys: [], mac: ['Ctrl+Q'], inInputs: true }
		]
	},
	{
		title: 'Edit',
		commands: [
			{ command: 'undo', label: 'Undo', keys: ['Ctrl+Z'] },
			{ command: 'redo', label: 'Redo', keys: ['Ctrl+Y', 'Ctrl+Shift+Z'], mac: ['Ctrl+Shift+Z', 'Ctrl+Y'] },
			{ command: 'copy', label: 'Copy selected text', keys: ['Ctrl+C'] },
			{ command: 'find', label: 'Find', keys: ['Ctrl+F'], inInputs: true },
			{ command: 'findNext', label: 'Find next', keys: ['F3'], mac: ['Ctrl+G', 'F3'], inInputs: true },
			{ command: 'findPrevious', label: 'Find previous', keys: ['Shift+F3'], mac: ['Ctrl+Shift+G', 'Shift+F3'], inInputs: true }
		]
	},
	{
		title: 'View',
		commands: [
			{ command: 'zoomIn', label: 'Zoom in', keys: ['Ctrl+='], aliases: ['Ctrl++', 'Ctrl+Shift++'], inInputs: true },
			{ command: 'zoomOut', label: 'Zoom out', keys: ['Ctrl+-'], inInputs: true },
			{ command: 'fitWidth', label: 'Fit width', keys: ['Ctrl+0'], inInputs: true },
			{ command: 'fitPage', label: 'Fit page', keys: [], inInputs: true },
			{ command: 'rotateViewClockwise', label: 'Rotate view clockwise', keys: [], inInputs: true },
			{ command: 'rotateViewCounterClockwise', label: 'Rotate view counter-clockwise', keys: [], inInputs: true },
			{ command: 'toggleCoverPage', label: 'Cover page alone', keys: [], inInputs: true },
			{ command: 'fullScreen', label: 'Full screen', keys: ['F11'], mac: ['Ctrl+Shift+F'], inInputs: true },
			{ command: 'toggleTheme', label: 'Switch between the light and dark theme', keys: [], inInputs: true },
			{ command: 'toggleLeftPane', label: 'Show or hide the left pane', keys: [], inInputs: true },
			{ command: 'toggleRightPane', label: 'Show or hide the right pane', keys: [], inInputs: true },
			{ command: 'showPages', label: 'Show or hide the Pages panel', keys: [], inInputs: true },
			{ command: 'showBookmarks', label: 'Show or hide the Bookmarks panel', keys: [], inInputs: true },
			{ command: 'showAnnotations', label: 'Show or hide the Annotations panel', keys: [], inInputs: true },
			{ command: 'showTwice', label: 'Show this document twice, side by side', keys: [], inInputs: true },
			{ command: 'showNextTabBeside', label: 'Show the next tab beside this one', keys: [], inInputs: true },
			{ command: 'separateTabs', label: 'Separate the documents side by side', keys: [], inInputs: true },
			{ command: 'syncScrolling', label: 'Synchronize scrolling side by side', keys: [], inInputs: true },
			{ command: 'nextPane', label: 'Next pane side by side', keys: [], inInputs: true },
			{ command: 'previousPane', label: 'Previous pane side by side', keys: [], inInputs: true }
		]
	},
	{
		title: 'Go to',
		commands: [
			{ command: 'goToPage', label: 'Go to page', keys: ['Ctrl+G'], mac: ['Ctrl+Alt+G'], inInputs: true },
			{ command: 'back', label: 'Back', keys: ['Alt+Left'], mac: ['Ctrl+[', 'Alt+Left'], aliases: ['BrowserBack'] },
			{ command: 'forward', label: 'Forward', keys: ['Alt+Right'], mac: ['Ctrl+]', 'Alt+Right'], aliases: ['BrowserForward'] },
			{ command: 'nextTab', label: 'Next tab', keys: ['Ctrl+Tab', 'Ctrl+Page Down'], inInputs: true },
			{ command: 'previousTab', label: 'Previous tab', keys: ['Ctrl+Shift+Tab', 'Ctrl+Page Up'], inInputs: true }
		]
	},
	{
		title: 'Document',
		commands: [
			{ command: 'addBookmark', label: 'Add bookmark', keys: ['Ctrl+B'] },
			{ command: 'organizePages', label: 'Organize pages', keys: [] },
			{ command: 'insertPages', label: 'Insert pages from file', keys: [] },
			{ command: 'showLabels', label: 'Page labels, while organizing pages', keys: [] },
			{ command: 'repairAnnotations', label: 'Repair annotations', keys: [] },
			{ command: 'documentProperties', label: 'Properties', keys: ['Ctrl+D'], inInputs: true }
		]
	},
	{
		title: 'Annotation tools',
		commands: [
			{ command: 'toolSelect', label: 'Select tool', keys: [] },
			{ command: 'toolHighlight', label: 'Highlight tool', keys: ['H'] },
			{ command: 'toolUnderline', label: 'Underline tool', keys: ['U'] },
			{ command: 'toolStrikeOut', label: 'Strikeout tool', keys: [] },
			{ command: 'toolSquiggly', label: 'Squiggly tool', keys: [] },
			{ command: 'toolNote', label: 'Note tool', keys: ['N'] },
			{ command: 'toolPen', label: 'Pen tool', keys: ['P'] },
			{ command: 'toolText', label: 'Text box tool', keys: ['T'] },
			{ command: 'colour1', label: 'Preset colour 1', keys: ['1'] },
			{ command: 'colour2', label: 'Preset colour 2', keys: ['2'] },
			{ command: 'colour3', label: 'Preset colour 3', keys: ['3'] },
			{ command: 'colour4', label: 'Preset colour 4', keys: ['4'] },
			{ command: 'colour5', label: 'Preset colour 5', keys: ['5'] },
			{ command: 'colour6', label: 'Preset colour 6', keys: ['6'] },
			{ command: 'colour7', label: 'Preset colour 7', keys: ['7'] },
			{ command: 'colour8', label: 'Preset colour 8', keys: ['8'] },
			{ command: 'colour9', label: 'Preset colour 9', keys: ['9'] }
		]
	},
	{
		title: 'Help',
		commands: [
			{ command: 'whatsNew', label: 'What’s new', keys: [], inInputs: true },
			{ command: 'checkForUpdates', label: 'Check for updates', keys: [], inInputs: true },
			{ command: 'about', label: 'About', keys: [], inInputs: true }
		]
	}
];

const COMMANDS: readonly ShortcutCommand[] = SHORTCUT_GROUPS.flatMap((g) => g.commands);
const BY_NAME = new Map(COMMANDS.map((c) => [c.command, c]));

/** The shortcuts a command comes with on the platform. */
export function defaultKeys(c: ShortcutCommand, platform: Platform = PLATFORM): readonly string[] {
	return platform === 'macos' ? (c.mac ?? c.keys) : c.keys;
}

/** The shortcuts the command named comes with on the platform. */
function defaultsOf(command: CommandName, platform: Platform = PLATFORM): readonly string[] {
	const c = BY_NAME.get(command);
	return c ? defaultKeys(c, platform) : [];
}

/** Keys that work where the focus is and can't be changed, listed in Settings with the rest. */
export const FIXED_KEYS: readonly { keys: string; label: string }[] = [
	{ keys: 'Esc', label: 'Back a step: what is being drawn, the selection, the tool, the search, full screen' },
	{ keys: 'Del', label: 'Delete the selected annotation, bookmark or pages' },
	{ keys: 'F2', label: 'Rename the selected bookmark' },
	{ keys: 'Shift+F10', label: 'Open the context menu' },
	{ keys: 'Arrows, Ctrl+Arrows', label: 'Move the text cursor a click put in the text' },
	{ keys: 'Shift+Arrows', label: 'Select more or less text, by a character or a line' },
	{ keys: 'Ctrl+Shift+Arrows', label: 'Select more or less text, by a word or a paragraph' },
	{ keys: 'Ctrl+Shift+Left, Right', label: 'Move the focused tab or panel along its row' },
	{ keys: 'Alt+Shift+Arrows', label: 'Move the selected bookmark, or the selected pages in Combine' },
	{ keys: 'Ctrl+Enter', label: 'Finish a comment, a reply or a text box' },
	{ keys: 'Alt (held)', label: 'In full screen, show the side panes and the toolbar' }
];

/** The most shortcuts one command can have. */
export const MAX_KEYS = 3;

/** Shortcuts changed in Settings: a command's name and its keys (none: no shortcut). */
export type ShortcutOverrides = Readonly<Record<string, readonly string[] | undefined>>;

// ----- how a shortcut is written -----

interface Chord {
	key: string;
	ctrl: boolean;
	alt: boolean;
	shift: boolean;
}

/** Keys by the name shown, from `KeyboardEvent.key`. */
const KEY_NAMES: Record<string, string> = {
	ArrowLeft: 'Left',
	ArrowRight: 'Right',
	ArrowUp: 'Up',
	ArrowDown: 'Down',
	' ': 'Space',
	PageDown: 'Page Down',
	PageUp: 'Page Up',
	Escape: 'Esc',
	Delete: 'Del'
};

/** Keys that are no shortcut by themselves. */
const NOT_KEYS = new Set(['Control', 'Shift', 'Alt', 'AltGraph', 'Meta', 'OS', 'CapsLock', 'NumLock', 'ScrollLock', 'Fn', 'Dead', 'Process', 'Unidentified']);

function format({ key, ctrl, alt, shift }: Chord): string {
	return `${ctrl ? 'Ctrl+' : ''}${alt ? 'Alt+' : ''}${shift ? 'Shift+' : ''}${key}`;
}

function parse(text: string): Chord | null {
	const m = /^((?:(?:Ctrl|Alt|Shift)\+)*)(.+)$/.exec(text);
	if (!m) return null;
	const key = m[2]!;
	const named = /^[A-Z][A-Za-z]*(?: [A-Z][a-z]+)?\d{0,2}$/.test(key);
	if (key.length !== 1 && !named) return null;
	if (key.length === 1 && (key === ' ' || key !== upper(key))) return null;
	const mods = m[1]!;
	return { key, ctrl: mods.includes('Ctrl+'), alt: mods.includes('Alt+'), shift: mods.includes('Shift+') };
}

/** A letter as a shortcut names it: upper case, where that is still one character. */
function upper(key: string): string {
	const u = key.toUpperCase();
	return u.length === 1 ? u : key;
}

/** Punctuation keys the bindings use, by physical key (`KeyboardEvent.code`). */
const PUNCTUATION_CODES: Record<string, string> = { Equal: '=', Minus: '-', Comma: ',' };

/**
 * The key a shortcut is matched against. Usually the character typed, so Latin layouts
 * other than QWERTY (AZERTY, Dvorak) match by their own letters. A key that types a
 * non-Latin character (Cyrillic, Greek, Hebrew...) matches by its place on the keyboard
 * instead, as on a US layout, the way Windows apps treat shortcuts: Ctrl+Я is Ctrl+Z.
 */
export function shortcutKey(event: Pick<KeyboardEvent, 'key' | 'code'>): string {
	const { key, code } = event;
	if (key.length !== 1 || /^[\x20-\x7e]$/.test(key) || !code) return key;
	if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
	if (/^Digit[0-9]$/.test(code)) return code.slice(5);
	return PUNCTUATION_CODES[code] ?? key;
}

type KeyPress = Pick<KeyboardEvent, 'key' | 'code' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>;

/** The shortcut a key press is, as written; null for a modifier key alone. */
export function chordOf(event: KeyPress): string | null {
	const pressed = shortcutKey(event);
	if (!pressed || NOT_KEYS.has(pressed)) return null;
	const key = KEY_NAMES[pressed] ?? (pressed.length === 1 ? upper(pressed) : pressed);
	return format({ key, ctrl: event.ctrlKey || event.metaKey, alt: event.altKey, shift: event.shiftKey });
}

const ARROWS = ['Left', 'Right', 'Up', 'Down'];
/** Keys that move around or edit wherever the focus is. */
const FOCUS_KEYS = ['Esc', 'Enter', 'Tab', 'Space', 'Backspace', 'Del', 'Insert', 'Home', 'End', 'Page Up', 'Page Down', ...ARROWS];

/**
 * Why a shortcut can't be given to a command: null if it can. Keys with a fixed use stay
 * out (`FIXED_KEYS`, the keys that move the focus or edit text, Alt+F4).
 */
export function reservedReason(text: string, platform: Platform = PLATFORM): string | null {
	const chord = parse(text);
	if (!chord) return 'That key can’t be used in a shortcut.';
	const { key, ctrl, alt, shift } = chord;
	// macOS's own: Hide, Hide others and Minimize, in every app's menus.
	if (platform === 'macos' && ctrl && !shift && (key === 'H' || (key === 'M' && !alt))) {
		return `${displayShortcut(text, platform)} belongs to macOS, so it can’t be a shortcut.`;
	}
	const fixed =
		(!ctrl && !alt && FOCUS_KEYS.includes(key)) ||
		key === 'Enter' ||
		key === 'ContextMenu' ||
		(ARROWS.includes(key) && shift) ||
		(key === 'F10' && shift && !ctrl && !alt) ||
		(key === 'F2' && !ctrl && !alt && !shift) ||
		(key === 'F4' && alt) ||
		(key === 'Tab' && alt) ||
		(ctrl && !alt && !shift && ['A', 'V', 'X'].includes(key));
	return fixed ? `${displayShortcut(text, platform)} already has a fixed use, so it can’t be a shortcut.` : null;
}

/** Shortcuts that edit text in a field, whatever command has them elsewhere. */
function editsText({ key, ctrl, alt }: Chord): boolean {
	if (!ctrl || alt) return false;
	return ['A', 'C', 'V', 'X', 'Z', 'Y', 'Backspace', 'Del', 'Home', 'End', ...ARROWS].includes(key);
}

/** Whether a shortcut can run a command while a text field has the focus: only with Ctrl
 * or Alt, or a function key, so nothing that types. */
function worksInInputs(chord: Chord): boolean {
	return (chord.ctrl || chord.alt || /^F\d{1,2}$/.test(chord.key)) && !editsText(chord);
}

// ----- how a shortcut is shown -----

/** Keys as macOS shows them in menus. */
const MAC_KEYS: Record<string, string> = {
	Left: '←',
	Right: '→',
	Up: '↑',
	Down: '↓',
	'Page Up': '⇞',
	'Page Down': '⇟',
	Home: '↖',
	End: '↘',
	Esc: '⎋',
	Del: '⌦',
	Backspace: '⌫',
	Enter: '↩',
	Tab: '⇥'
};

/**
 * A shortcut as the platform shows it: as written on Windows and Linux; on macOS with the
 * system's symbols, the modifiers in its order (⌃⌥⇧⌘) before the key, Ctrl as ⌘ (as it
 * answers to the Command key), except with Tab, where Command switches apps and the
 * Control key it answers to is meant: "Ctrl+Shift+S" is ⇧⌘S, "Ctrl+Tab" ⌃⇥. Text that is
 * not one shortcut (the fixed keys' "Ctrl+Shift+Left, Right") has its modifiers shown so.
 */
export function displayShortcut(text: string, platform: Platform = PLATFORM): string {
	if (platform !== 'macos' || !text) return text;
	const m = /^((?:(?:Ctrl|Alt|Shift)\+)*)(.+)$/.exec(text);
	if (!m) return text;
	const mods = m[1]!;
	let key = m[2]!;
	let alt = mods.includes('Alt+');
	if (!mods && /^Alt\b/.test(key)) {
		alt = true;
		key = key.slice(3);
	}
	const ctrl = mods.includes('Ctrl+');
	const control = ctrl && key === 'Tab';
	const symbols = `${control ? '⌃' : ''}${alt ? '⌥' : ''}${mods.includes('Shift+') ? '⇧' : ''}${ctrl && !control ? '⌘' : ''}`;
	return symbols + (MAC_KEYS[key] ?? key);
}

/** Keys as muda's accelerators name them, where Lectrix writes them otherwise. */
const ACCELERATOR_KEYS: Record<string, string> = { 'Page Up': 'PageUp', 'Page Down': 'PageDown', Del: 'Delete', Esc: 'Escape' };

/**
 * A shortcut as the system's menu bar on macOS takes it (muda's accelerators: "Cmd+Shift+S"),
 * with Ctrl as Command but with Tab (see `displayShortcut`); null for one it can't show.
 */
export function menuAccelerator(text: string): string | null {
	const chord = parse(text);
	if (!chord) return null;
	const { key, ctrl, alt, shift } = chord;
	const mods = [ctrl && (key === 'Tab' ? 'Ctrl' : 'Cmd'), alt && 'Alt', shift && 'Shift'].filter(Boolean);
	return [...mods, ACCELERATOR_KEYS[key] ?? key].join('+');
}

// ----- the keys in use -----

/** Valid shortcuts out of stored ones: written as shown, not reserved, each once. */
function clean(keys: readonly string[]): string[] {
	const out: string[] = [];
	for (const text of keys) {
		const chord = parse(text);
		if (!chord) continue;
		const written = format(chord);
		if (reservedReason(written) === null && !out.includes(written)) out.push(written);
	}
	return out.slice(0, MAX_KEYS);
}

/** A command's shortcuts: Settings' own, or the ones it comes with. */
export function keysOf(command: CommandName, overrides?: ShortcutOverrides | null, platform: Platform = PLATFORM): readonly string[] {
	const own = overrides?.[command];
	return own ? clean(own) : defaultsOf(command, platform);
}

/** A command's first shortcut, for menus and tooltips; '' without one. */
export function shortcutOf(command: CommandName, overrides?: ShortcutOverrides | null, platform: Platform = PLATFORM): string {
	return keysOf(command, overrides, platform)[0] ?? '';
}

/** Every shortcut a command answers to, the unlisted ones included while it has its own keys. */
function allKeys(c: ShortcutCommand, overrides?: ShortcutOverrides | null): readonly string[] {
	return overrides?.[c.command] ? keysOf(c.command, overrides) : [...defaultKeys(c), ...(c.aliases ?? [])];
}

/** The command that has a shortcut, other than `except`; null if it is free. */
export function commandUsing(text: string, overrides?: ShortcutOverrides | null, except?: CommandName): ShortcutCommand | null {
	return COMMANDS.find((c) => c.command !== except && allKeys(c, overrides).includes(text)) ?? null;
}

/**
 * The overrides with `command` given `keys`. Keys equal to the ones it comes with are
 * not stored, so the command follows the defaults again.
 */
export function withKeys(overrides: ShortcutOverrides | null | undefined, command: CommandName, keys: readonly string[]): Record<string, string[]> {
	const next: Record<string, string[]> = {};
	for (const [name, own] of Object.entries(overrides ?? {})) if (own && name !== command) next[name] = [...own];
	const cleaned = clean(keys);
	const defaults = defaultsOf(command);
	if (cleaned.length !== defaults.length || cleaned.some((k, i) => k !== defaults[i])) next[command] = cleaned;
	return next;
}

/** True if Settings changed the command's shortcuts. */
export function isChanged(command: CommandName, overrides?: ShortcutOverrides | null): boolean {
	return overrides?.[command] !== undefined;
}

interface Live {
	command: CommandName;
	inInputs: boolean;
}

/** Shortcut to command. Where two commands were stored with one shortcut, the first has it. */
function table(overrides?: ShortcutOverrides | null): Map<string, Live> {
	const live = new Map<string, Live>();
	for (const c of COMMANDS) {
		for (const text of allKeys(c, overrides)) {
			if (!live.has(text)) live.set(text, { command: c.command, inInputs: !!c.inInputs });
		}
	}
	return live;
}

let live = table();

/** Applies the shortcuts changed in Settings (and, with none, the defaults). */
export function setShortcuts(overrides?: ShortcutOverrides | null) {
	live = table(overrides);
}

/** The command for a key event, or null. */
export function commandFor(event: KeyPress, inInput: boolean): CommandName | null {
	const text = chordOf(event);
	const hit = text ? live.get(text) : undefined;
	if (!text || !hit) return null;
	if (inInput) {
		const chord = parse(text);
		if (!hit.inInputs || !chord || !worksInInputs(chord)) return null;
	}
	return hit.command;
}

/**
 * For key handlers of text fields inside lists and panels: keeps the key from reaching the
 * list's own keys (Delete, arrows) and single-letter tool keys, but lets app-wide
 * shortcuts that work in text fields (Ctrl+S, Ctrl+F...) through to the window.
 */
export function stopUnlessShortcut(event: KeyboardEvent) {
	if (!commandFor(event, true)) event.stopPropagation();
}

/** Commands that act on the document as saved: a text field commits its edit first. */
export const COMMIT_FIELD_FIRST: ReadonlySet<CommandName> = new Set(['save', 'saveAs', 'closeTab']);

// ----- the webview's own keys -----

/** Browser shortcuts that make no sense in the app (reload, print preview, view source). */
const BLOCKED: { key: string; ctrl?: boolean; shift?: boolean }[] = [
	{ key: 'p', ctrl: true },
	{ key: 'u', ctrl: true },
	{ key: 'j', ctrl: true },
	{ key: 'i', ctrl: true, shift: true }
];

function sameKey(a: string, b: string) {
	return a.length === 1 && b.length === 1 ? a.toLowerCase() === b.toLowerCase() : a === b;
}

/** Reload: Ctrl+R and F5, with or without Shift or Ctrl (the hard-reload variants). */
function isReload(event: KeyboardEvent): boolean {
	return event.key === 'F5' || ((event.ctrlKey || event.metaKey) && sameKey('r', shortcutKey(event)));
}

/**
 * Browser keys the app swallows, unless Settings gave one to a command. Development builds
 * keep reload, so a change can be picked up without restarting `tauri dev` (open documents
 * come back after a reload).
 */
export function isBlocked(event: KeyboardEvent, allowReload: boolean = import.meta.env.DEV): boolean {
	if (isReload(event)) return !allowReload;
	const ctrl = event.ctrlKey || event.metaKey;
	const pressed = shortcutKey(event);
	return BLOCKED.some((b) => sameKey(b.key, pressed) && !!b.ctrl === ctrl && !!b.shift === event.shiftKey);
}

/**
 * The webview's own page zoom (Ctrl with +, =, -, _ or 0, main keyboard or keypad). It is
 * switched on so touchpad pinches reach the viewer, and must never scale the app itself;
 * the zoom commands still run through the shortcuts above.
 */
export function isBrowserZoomKey(event: KeyboardEvent): boolean {
	return (event.ctrlKey || event.metaKey) && !event.altKey && ['=', '+', '-', '_', '0'].includes(shortcutKey(event));
}

export function isTextInput(target: EventTarget | null): boolean {
	if (!(target instanceof HTMLElement)) return false;
	return target.isContentEditable || target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT';
}
