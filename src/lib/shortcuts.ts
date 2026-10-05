// Default keyboard shortcuts (section 8). Rebinding comes after v1.

import type { CommandName } from './commands.ts';

interface Binding {
	key: string;
	ctrl?: boolean;
	shift?: boolean;
	alt?: boolean;
	command: CommandName;
	/** Also active while typing in a text field. */
	inInputs?: boolean;
}

const BINDINGS: Binding[] = [
	{ key: 'o', ctrl: true, command: 'open', inInputs: true },
	{ key: 's', ctrl: true, command: 'save', inInputs: true },
	{ key: 's', ctrl: true, shift: true, command: 'saveAs', inInputs: true },
	{ key: 'w', ctrl: true, command: 'closeTab', inInputs: true },
	{ key: 'F4', ctrl: true, command: 'closeTab', inInputs: true },
	{ key: 'z', ctrl: true, command: 'undo' },
	{ key: 'y', ctrl: true, command: 'redo' },
	{ key: 'z', ctrl: true, shift: true, command: 'redo' },
	{ key: 'c', ctrl: true, command: 'copy' },
	{ key: 'f', ctrl: true, command: 'find', inInputs: true },
	{ key: 'F3', command: 'findNext', inInputs: true },
	{ key: 'F3', shift: true, command: 'findPrevious', inInputs: true },
	{ key: 'g', ctrl: true, command: 'goToPage', inInputs: true },
	{ key: 'b', ctrl: true, command: 'addBookmark' },
	{ key: '=', ctrl: true, command: 'zoomIn', inInputs: true },
	{ key: '+', ctrl: true, command: 'zoomIn', inInputs: true },
	{ key: '+', ctrl: true, shift: true, command: 'zoomIn', inInputs: true },
	{ key: '-', ctrl: true, command: 'zoomOut', inInputs: true },
	{ key: '0', ctrl: true, command: 'fitWidth', inInputs: true },
	{ key: 'ArrowLeft', alt: true, command: 'back' },
	{ key: 'ArrowRight', alt: true, command: 'forward' },
	{ key: 'BrowserBack', command: 'back' },
	{ key: 'BrowserForward', command: 'forward' },
	{ key: 'Tab', ctrl: true, command: 'nextTab', inInputs: true },
	{ key: 'Tab', ctrl: true, shift: true, command: 'previousTab', inInputs: true },
	{ key: 'PageDown', ctrl: true, command: 'nextTab', inInputs: true },
	{ key: 'PageUp', ctrl: true, command: 'previousTab', inInputs: true },
	{ key: ',', ctrl: true, command: 'settings', inInputs: true },
	// Annotation tools (Esc for Select is handled with the other uses of Escape).
	{ key: 'h', command: 'toolHighlight' },
	{ key: 'u', command: 'toolUnderline' },
	{ key: 'n', command: 'toolNote' },
	{ key: 'p', command: 'toolPen' },
	{ key: 't', command: 'toolText' }
];

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

/** The command for a key event, or null. */
export function commandFor(event: KeyboardEvent, inInput: boolean): CommandName | null {
	const ctrl = event.ctrlKey || event.metaKey;
	const pressed = shortcutKey(event);
	for (const b of BINDINGS) {
		if (
			sameKey(b.key, pressed) &&
			!!b.ctrl === ctrl &&
			!!b.shift === event.shiftKey &&
			!!b.alt === event.altKey &&
			(!inInput || b.inInputs)
		) {
			return b.command;
		}
	}
	return null;
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

/** Reload: Ctrl+R and F5, with or without Shift or Ctrl (the hard-reload variants). */
function isReload(event: KeyboardEvent): boolean {
	return event.key === 'F5' || ((event.ctrlKey || event.metaKey) && sameKey('r', shortcutKey(event)));
}

/**
 * Browser keys the app swallows. Development builds keep reload, so a change can be
 * picked up without restarting `tauri dev` (open documents come back after a reload).
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
 * the zoom commands still run through the bindings above.
 */
export function isBrowserZoomKey(event: KeyboardEvent): boolean {
	return (event.ctrlKey || event.metaKey) && !event.altKey && ['=', '+', '-', '_', '0'].includes(shortcutKey(event));
}

export function isTextInput(target: EventTarget | null): boolean {
	if (!(target instanceof HTMLElement)) return false;
	return target.isContentEditable || target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT';
}
