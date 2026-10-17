import { afterEach, describe, expect, it } from 'vitest';

import {
	COMMIT_FIELD_FIRST,
	FIXED_KEYS,
	MAX_KEYS,
	SHORTCUT_GROUPS,
	chordOf,
	commandFor,
	commandUsing,
	defaultKeys,
	displayShortcut,
	isBlocked,
	isBrowserZoomKey,
	isChanged,
	keysOf,
	menuAccelerator,
	reservedReason,
	setShortcuts,
	shortcutOf,
	stopUnlessShortcut,
	withKeys
} from './shortcuts';

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

describe('shortcuts changed in Settings', () => {
	afterEach(() => setShortcuts(null));

	const all = SHORTCUT_GROUPS.flatMap((g) => g.commands);

	it('lists every command once, with defaults that are written as shown and free to use', () => {
		expect(new Set(all.map((c) => c.command)).size).toBe(all.length);
		const seen = new Map<string, string>();
		for (const c of all) {
			expect(c.keys.length, c.command).toBeLessThanOrEqual(MAX_KEYS);
			// Written as Settings writes them: storing them changes nothing.
			expect(keysOf(c.command, { [c.command]: c.keys }), c.command).toEqual(c.keys);
			for (const k of [...c.keys, ...(c.aliases ?? [])]) {
				expect(reservedReason(k), `${c.command} ${k}`).toBeNull();
				expect(seen.get(k), `${k} for ${c.command}`).toBeUndefined();
				seen.set(k, c.command);
			}
		}
		expect(FIXED_KEYS.length).toBeGreaterThan(0);
	});

	it('reads a key press as Settings writes it', () => {
		expect(chordOf(key('s', { ctrl: true }))).toBe('Ctrl+S');
		expect(chordOf(key('S', { ctrl: true, shift: true }))).toBe('Ctrl+Shift+S');
		expect(chordOf(key('ArrowLeft', { alt: true }))).toBe('Alt+Left');
		expect(chordOf(key('PageDown', { ctrl: true }))).toBe('Ctrl+Page Down');
		expect(chordOf(key('F3', { shift: true }))).toBe('Shift+F3');
		expect(chordOf(key('=', { ctrl: true }))).toBe('Ctrl+=');
		expect(chordOf(key('k', { ctrl: true, alt: true, shift: true }))).toBe('Ctrl+Alt+Shift+K');
		expect(chordOf(key('h'))).toBe('H');
		// A non-Latin layout: by the key's place, as the shortcuts are matched.
		expect(chordOf(key('я', { ctrl: true, code: 'KeyZ' }))).toBe('Ctrl+Z');
		// A modifier alone is no shortcut yet.
		expect(chordOf(key('Control', { ctrl: true }))).toBeNull();
		expect(chordOf(key('Shift', { shift: true }))).toBeNull();
	});

	it('runs a command from its new shortcut, and no longer from the old one', () => {
		const overrides = withKeys(null, 'save', ['Ctrl+Alt+S']);
		setShortcuts(overrides);
		expect(commandFor(key('s', { ctrl: true, alt: true }), false)).toBe('save');
		expect(commandFor(key('s', { ctrl: true }), false)).toBeNull();
		expect(shortcutOf('save', overrides)).toBe('Ctrl+Alt+S');
		// The others keep theirs.
		expect(commandFor(key('o', { ctrl: true }), false)).toBe('open');
		// Back to the defaults.
		setShortcuts({});
		expect(commandFor(key('s', { ctrl: true }), false)).toBe('save');
	});

	it('gives a command without one a shortcut, takes one away, and adds a second', () => {
		let overrides = withKeys(null, 'fitPage', ['Ctrl+9']);
		overrides = withKeys(overrides, 'copy', []);
		overrides = withKeys(overrides, 'find', ['Ctrl+F', 'F6']);
		setShortcuts(overrides);
		expect(commandFor(key('9', { ctrl: true }), false)).toBe('fitPage');
		expect(commandFor(key('c', { ctrl: true }), false)).toBeNull();
		expect(shortcutOf('copy', overrides)).toBe('');
		expect(commandFor(key('F6'), false)).toBe('find');
		expect(commandFor(key('f', { ctrl: true }), false)).toBe('find');
		expect(isChanged('copy', overrides)).toBe(true);
		expect(isChanged('open', overrides)).toBe(false);
	});

	it('stores nothing for a command put back to the keys it came with', () => {
		const overrides = withKeys(withKeys(null, 'redo', ['Ctrl+Shift+Y']), 'redo', ['Ctrl+Y', 'Ctrl+Shift+Z']);
		expect(overrides).toEqual({});
		expect(isChanged('redo', overrides)).toBe(false);
	});

	it('keeps the unlisted keys of a command only while it has its own', () => {
		// The keypad's plus zooms in, with the default.
		expect(commandFor(key('+', { ctrl: true }), false)).toBe('zoomIn');
		expect(commandUsing('Ctrl++', null)?.command).toBe('zoomIn');
		const overrides = withKeys(null, 'zoomIn', ['Ctrl+I']);
		setShortcuts(overrides);
		expect(commandFor(key('+', { ctrl: true }), false)).toBeNull();
		expect(commandUsing('Ctrl++', overrides)).toBeNull();
	});

	it('finds the command that already has a shortcut', () => {
		expect(commandUsing('Ctrl+S', null)?.command).toBe('save');
		expect(commandUsing('Ctrl+S', null, 'save')).toBeNull();
		expect(commandUsing('Ctrl+Alt+S', null)).toBeNull();
		const overrides = withKeys(null, 'save', ['Ctrl+Alt+S']);
		expect(commandUsing('Ctrl+S', overrides)).toBeNull();
		expect(commandUsing('Ctrl+Alt+S', overrides)?.label).toBe('Save');
	});

	it('refuses keys with a fixed use', () => {
		const fixed = ['Esc', 'Enter', 'Tab', 'Space', 'Del', 'Left', 'Shift+Down', 'Page Down', 'Home', 'Ctrl+Enter'];
		for (const k of [...fixed, 'Ctrl+Shift+Left', 'Alt+Shift+Up', 'Shift+F10', 'F2', 'Alt+F4', 'Ctrl+V', 'Ctrl+A']) {
			expect(reservedReason(k), k).not.toBeNull();
		}
		for (const k of ['Ctrl+Alt+S', 'F6', 'Ctrl+Left', 'Alt+Left', 'Ctrl+Page Down', 'Q', 'Shift+H', 'Ctrl+P', 'Ctrl+Shift+V']) {
			expect(reservedReason(k), k).toBeNull();
		}
	});

	it('drops stored shortcuts it cannot use', () => {
		const stored = { save: ['ctrl+s', 'Shift+Ctrl+S', 'Esc', 'Ctrl+Alt+S', 'Ctrl+Alt+S', 'F7', 'F8', 'F9'], nonsense: ['Ctrl+Q'] };
		// Lower-case letters and fixed keys go, the order of modifiers is put right, each key
		// once, three at most.
		expect(keysOf('save', stored)).toEqual(['Ctrl+Shift+S', 'Ctrl+Alt+S', 'F7']);
		setShortcuts(stored);
		expect(commandFor(key('q', { ctrl: true }), false)).toBeNull();
		// Two commands stored with one shortcut: the first listed has it.
		setShortcuts({ save: ['Ctrl+O'] });
		expect(commandFor(key('o', { ctrl: true }), false)).toBe('open');
	});

	it('never runs a command from a key that types or edits text while a field has the focus', () => {
		setShortcuts({ save: ['S', 'Ctrl+C', 'F6'], reload: ['Alt+S'], toolPen: ['Ctrl+Alt+P'] });
		expect(commandFor(key('s'), false)).toBe('save');
		expect(commandFor(key('s'), true)).toBeNull();
		expect(commandFor(key('c', { ctrl: true }), false)).toBe('save');
		expect(commandFor(key('c', { ctrl: true }), true)).toBeNull();
		expect(commandFor(key('F6'), true)).toBe('save');
		expect(commandFor(key('s', { alt: true }), true)).toBe('reload');
		// A command that leaves text fields alone still does, whatever its key.
		expect(commandFor(key('p', { ctrl: true, alt: true }), false)).toBe('toolPen');
		expect(commandFor(key('p', { ctrl: true, alt: true }), true)).toBeNull();
	});
});

describe('shortcuts on macOS', () => {
	const all = SHORTCUT_GROUPS.flatMap((g) => g.commands);

	it('come with keys that are written as shown, free to use there, and each once', () => {
		const seen = new Map<string, string>();
		for (const c of all) {
			const keys = defaultKeys(c, 'macos');
			expect(keys.length, c.command).toBeLessThanOrEqual(MAX_KEYS);
			for (const k of [...keys, ...(c.aliases ?? [])]) {
				expect(reservedReason(k, 'macos'), `${c.command} ${k}`).toBeNull();
				expect(menuAccelerator(k) ?? (c.aliases?.includes(k) ? 'alias' : null), `${c.command} ${k}`).not.toBeNull();
				expect(seen.get(k), `${k} for ${c.command}`).toBeUndefined();
				seen.set(k, c.command);
			}
		}
		const byName = new Map(all.map((c) => [c.command, c]));
		expect(defaultKeys(byName.get('redo')!, 'macos')[0]).toBe('Ctrl+Shift+Z');
		expect(defaultKeys(byName.get('redo')!, 'windows')[0]).toBe('Ctrl+Y');
	});

	it('leave the keys of the system’s own menu items alone', () => {
		expect(reservedReason('Ctrl+H', 'macos')).toMatch(/belongs to macOS/);
		expect(reservedReason('Ctrl+Alt+H', 'macos')).toMatch(/belongs to macOS/);
		expect(reservedReason('Ctrl+M', 'macos')).toMatch(/belongs to macOS/);
		expect(reservedReason('Ctrl+H', 'windows')).toBeNull();
	});

	it('are shown as the system shows them', () => {
		expect(displayShortcut('Ctrl+Shift+S', 'macos')).toBe('⇧⌘S');
		expect(displayShortcut('Ctrl+Alt+G', 'macos')).toBe('⌥⌘G');
		expect(displayShortcut('Alt+Left', 'macos')).toBe('⌥←');
		expect(displayShortcut('Ctrl+Tab', 'macos')).toBe('⌃⇥');
		expect(displayShortcut('Ctrl+Shift+Tab', 'macos')).toBe('⌃⇧⇥');
		expect(displayShortcut('Ctrl+Page Down', 'macos')).toBe('⌘⇟');
		expect(displayShortcut('F3', 'macos')).toBe('F3');
		expect(displayShortcut('Ctrl+Shift+Left, Right', 'macos')).toBe('⇧⌘Left, Right');
		expect(displayShortcut('Alt (held)', 'macos')).toBe('⌥ (held)');
		expect(displayShortcut('Ctrl+Shift+S', 'windows')).toBe('Ctrl+Shift+S');
	});

	it('are given to the menu bar as muda reads them', () => {
		expect(menuAccelerator('Ctrl+Shift+S')).toBe('Cmd+Shift+S');
		expect(menuAccelerator('Ctrl+Tab')).toBe('Ctrl+Tab');
		expect(menuAccelerator('Ctrl+Page Down')).toBe('Cmd+PageDown');
		expect(menuAccelerator('Ctrl+=')).toBe('Cmd+=');
		expect(menuAccelerator('Alt+Left')).toBe('Alt+Left');
		expect(menuAccelerator('Shift+F3')).toBe('Shift+F3');
	});
});
