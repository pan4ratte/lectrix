import { describe, expect, it } from 'vitest';

import { COMMIT_FIELD_FIRST, commandFor, isBlocked, isBrowserZoomKey, stopUnlessShortcut } from './shortcuts';

function key(k: string, mods: { ctrl?: boolean; shift?: boolean; alt?: boolean; code?: string } = {}) {
	return {
		key: k,
		code: mods.code ?? '',
		ctrlKey: !!mods.ctrl,
		metaKey: false,
		shiftKey: !!mods.shift,
		altKey: !!mods.alt
	} as KeyboardEvent;
}

describe('shortcuts', () => {
	it('maps the default table from section 8', () => {
		expect(commandFor(key('o', { ctrl: true }), false)).toBe('open');
		expect(commandFor(key('S', { ctrl: true, shift: true }), false)).toBe('saveAs');
		expect(commandFor(key('s', { ctrl: true }), false)).toBe('save');
		expect(commandFor(key('z', { ctrl: true }), false)).toBe('undo');
		expect(commandFor(key('y', { ctrl: true }), false)).toBe('redo');
		expect(commandFor(key('f', { ctrl: true }), false)).toBe('find');
		expect(commandFor(key('g', { ctrl: true }), false)).toBe('goToPage');
		expect(commandFor(key('=', { ctrl: true }), false)).toBe('zoomIn');
		expect(commandFor(key('-', { ctrl: true }), false)).toBe('zoomOut');
		expect(commandFor(key('0', { ctrl: true }), false)).toBe('fitWidth');
		expect(commandFor(key('Tab', { ctrl: true }), false)).toBe('nextTab');
		expect(commandFor(key('Tab', { ctrl: true, shift: true }), false)).toBe('previousTab');
		expect(commandFor(key('ArrowLeft', { alt: true }), false)).toBe('back');
	});

	it('matches non-Latin layouts by the key pressed, Latin ones by the letter typed', () => {
		// Russian: the Z key types "я", the H key "р", the comma key "б".
		expect(commandFor(key('я', { ctrl: true, code: 'KeyZ' }), false)).toBe('undo');
		expect(commandFor(key('Я', { ctrl: true, shift: true, code: 'KeyZ' }), false)).toBe('redo');
		expect(commandFor(key('н', { ctrl: true, code: 'KeyY' }), false)).toBe('redo');
		expect(commandFor(key('ы', { ctrl: true, code: 'KeyS' }), true)).toBe('save');
		expect(commandFor(key('б', { ctrl: true, code: 'Comma' }), false)).toBe('settings');
		expect(commandFor(key('р', { code: 'KeyH' }), false)).toBe('toolHighlight');
		expect(commandFor(key('я', { ctrl: true, code: 'KeyZ' }), true)).toBeNull();
		expect(isBlocked(key('к', { ctrl: true, code: 'KeyR' }), false)).toBe(true);
		expect(isBlocked(key('з', { ctrl: true, code: 'KeyP' }))).toBe(true);
		// Greek.
		expect(commandFor(key('ζ', { ctrl: true, code: 'KeyZ' }), false)).toBe('undo');
		// Dvorak: "z" sits on the slash key, and the Z key types ";".
		expect(commandFor(key('z', { ctrl: true, code: 'Slash' }), false)).toBe('undo');
		expect(commandFor(key(';', { ctrl: true, code: 'KeyZ' }), false)).toBeNull();
	});

	it('leaves editing keys to text fields', () => {
		expect(commandFor(key('z', { ctrl: true }), true)).toBeNull();
		expect(commandFor(key('c', { ctrl: true }), true)).toBeNull();
		expect(commandFor(key('s', { ctrl: true }), true)).toBe('save');
		expect(commandFor(key('a'), false)).toBeNull();
	});

	it('blocks browser reload and print', () => {
		expect(isBlocked(key('r', { ctrl: true }), false)).toBe(true);
		expect(isBlocked(key('F5'), false)).toBe(true);
		expect(isBlocked(key('p', { ctrl: true }), false)).toBe(true);
		expect(isBlocked(key('s', { ctrl: true }), false)).toBe(false);
	});

	it('blocks the hard-reload variants too', () => {
		expect(isBlocked(key('R', { ctrl: true, shift: true }), false)).toBe(true);
		expect(isBlocked(key('F5', { ctrl: true }), false)).toBe(true);
		expect(isBlocked(key('F5', { shift: true }), false)).toBe(true);
		expect(isBlocked(key('r'), false)).toBe(false);
	});

	it('lets development builds reload, and still blocks the rest', () => {
		expect(isBlocked(key('r', { ctrl: true }), true)).toBe(false);
		expect(isBlocked(key('F5'), true)).toBe(false);
		expect(isBlocked(key('R', { ctrl: true, shift: true }), true)).toBe(false);
		expect(isBlocked(key('p', { ctrl: true }), true)).toBe(true);
	});

	it('recognizes every key that zooms the webview itself', () => {
		for (const k of ['=', '+', '-', '_', '0']) {
			expect(isBrowserZoomKey(key(k, { ctrl: true }))).toBe(true);
			expect(isBrowserZoomKey(key(k, { ctrl: true, shift: true }))).toBe(true);
			expect(isBrowserZoomKey(key(k))).toBe(false);
		}
		expect(isBrowserZoomKey(key('0', { ctrl: true, alt: true }))).toBe(false);
		expect(isBrowserZoomKey(key('s', { ctrl: true }))).toBe(false);
	});

	it('lets app-wide shortcuts out of panel text fields, and nothing else', () => {
		const stopped = (event: KeyboardEvent) => {
			let stop = false;
			stopUnlessShortcut({ ...event, stopPropagation: () => (stop = true) } as KeyboardEvent);
			return stop;
		};
		for (const k of [key('s', { ctrl: true }), key('f', { ctrl: true }), key('g', { ctrl: true })]) {
			expect(stopped(k), k.key).toBe(false);
		}
		for (const k of [key('Delete'), key('ArrowDown'), key('h'), key('Escape'), key('z', { ctrl: true })]) {
			expect(stopped(k), k.key).toBe(true);
		}
		expect([...COMMIT_FIELD_FIRST].sort()).toEqual(['closeTab', 'save', 'saveAs']);
	});
});
