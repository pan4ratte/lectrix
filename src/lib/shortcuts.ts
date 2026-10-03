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
	{ key: 'r', ctrl: true },
	{ key: 'F5' },
	{ key: 'p', ctrl: true },
	{ key: 'u', ctrl: true },
	{ key: 'j', ctrl: true },
	{ key: 'i', ctrl: true, shift: true }
];

function sameKey(a: string, b: string) {
	return a.length === 1 && b.length === 1 ? a.toLowerCase() === b.toLowerCase() : a === b;
}

/** The command for a key event, or null. */
export function commandFor(event: KeyboardEvent, inInput: boolean): CommandName | null {
	const ctrl = event.ctrlKey || event.metaKey;
	for (const b of BINDINGS) {
		if (
			sameKey(b.key, event.key) &&
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

export function isBlocked(event: KeyboardEvent): boolean {
	const ctrl = event.ctrlKey || event.metaKey;
	return BLOCKED.some((b) => sameKey(b.key, event.key) && !!b.ctrl === ctrl && !!b.shift === event.shiftKey);
}

export function isTextInput(target: EventTarget | null): boolean {
	if (!(target instanceof HTMLElement)) return false;
	return target.isContentEditable || target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT';
}
