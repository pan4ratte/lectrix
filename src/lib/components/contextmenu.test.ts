import { describe, expect, it } from 'vitest';

import { isContextMenuKey } from './contextmenu';

const key = (k: string, mods: Partial<KeyboardEvent> = {}) =>
	({ key: k, shiftKey: false, ctrlKey: false, altKey: false, ...mods }) as KeyboardEvent;

describe('context menu keys', () => {
	it('are the Menu key and Shift+F10', () => {
		expect(isContextMenuKey(key('ContextMenu'))).toBe(true);
		expect(isContextMenuKey(key('F10', { shiftKey: true }))).toBe(true);
		expect(isContextMenuKey(key('F10'))).toBe(false);
		expect(isContextMenuKey(key('F10', { shiftKey: true, ctrlKey: true }))).toBe(false);
	});
});
