import { describe, expect, it } from 'vitest';

import { commandFor, isBlocked } from './shortcuts';

function key(k: string, mods: { ctrl?: boolean; shift?: boolean; alt?: boolean } = {}) {
	return {
		key: k,
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

	it('leaves editing keys to text fields', () => {
		expect(commandFor(key('z', { ctrl: true }), true)).toBeNull();
		expect(commandFor(key('c', { ctrl: true }), true)).toBeNull();
		expect(commandFor(key('s', { ctrl: true }), true)).toBe('save');
		expect(commandFor(key('a'), false)).toBeNull();
	});

	it('blocks browser reload and print', () => {
		expect(isBlocked(key('r', { ctrl: true }))).toBe(true);
		expect(isBlocked(key('F5'))).toBe(true);
		expect(isBlocked(key('p', { ctrl: true }))).toBe(true);
		expect(isBlocked(key('s', { ctrl: true }))).toBe(false);
	});
});
