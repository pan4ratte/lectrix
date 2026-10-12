import { describe, expect, it } from 'vitest';

import type { Settings } from '#lib/ipc/index.ts';

import { RESET_ROW, SETTING_GROUPS, toDraft, toInput, type SettingRow } from './schema.ts';

/** Every stored setting, as Rust sends them (a new field in Rust fails to compile here). */
const STORED: Settings = {
	author: 'Ada',
	defaultAuthor: 'ada',
	appearance: 'system',
	scrollMode: 'singlePageContinuous',
	toolbarStyle: 'floating',
	toolbarDock: 'top',
	toolbarPosition: 'bottom',
	toolbarVisibility: 'always',
	quickTools: ['highlight', 'copy'],
	checkForUpdates: true,
	smoothZoom: true,
	smoothAnnotationScroll: true,
	smoothPageTurn: true,
	smoothBookmarkScroll: true,
	tooltipDelayMs: 300,
	openCommentAfterMarkup: false,
	rememberAnnotationStyle: true,
	selectionHandles: false,
	pdfaReadOnly: true,
	highlightLinks: true,
	linkPreviews: true,
	colorPanelOnHover: true,
	presetColors: { 2: '#12ab34' },
	shortcuts: { save: ['Ctrl+Alt+S'], copy: [] }
};

const rows = SETTING_GROUPS.flatMap((g) => g.sections.flatMap((s) => s.rows));
const row = (id: string): SettingRow => {
	const found = rows.find((r) => r.id === id);
	if (!found) throw new Error(`no row ${id}`);
	return found;
};

describe('settings schema', () => {
	it('has a row for every setting, and unique ids', () => {
		// A setting shows as a row, or in a group's own panel (the shortcuts).
		const shown = new Set<string>([...rows.flatMap((r) => ('key' in r ? [r.key] : [])), ...SETTING_GROUPS.flatMap((g) => (g.panel ? [g.panel.key] : []))]);
		for (const key of Object.keys(toInput(toDraft(STORED)))) expect(shown, key).toContain(key);
		expect(new Set(rows.map((r) => r.id)).size).toBe(rows.length);
		expect(new Set(SETTING_GROUPS.map((g) => g.id)).size).toBe(SETTING_GROUPS.length);
	});

	it('has no empty group or section', () => {
		for (const g of SETTING_GROUPS) {
			if (!g.panel) expect(g.sections.length, g.id).toBeGreaterThan(0);
			for (const s of g.sections) expect(s.rows.length, `${g.id} ${s.title ?? ''}`).toBeGreaterThan(0);
		}
	});

	it('leaves the author empty when it is the default, and trims it on save', () => {
		expect(toDraft({ ...STORED, author: 'ada' }).author).toBe('');
		const draft = toDraft(STORED);
		const author = row('author');
		if (author.kind !== 'text') throw new Error('author is not a text field');
		author.write(draft, '  Grace  ');
		expect(toInput(draft).author).toBe('Grace');
	});

	it('keeps the preset colours that were changed, as a copy', () => {
		const draft = toDraft(STORED);
		expect(draft.presetColors).toEqual({ 2: '#12ab34' });
		expect(draft.presetColors).not.toBe(STORED.presetColors);
		const presets = row('presetColors');
		if (presets.kind !== 'presets') throw new Error('presetColors is not a presets row');
		presets.write(draft, { ...presets.read(draft), 5: '#abcdef' });
		expect(toInput(draft).presetColors).toEqual({ 2: '#12ab34', 5: '#abcdef' });
		// Restoring them is offered only while some are changed.
		const restore = row('resetPresetColors');
		expect(restore.disabled?.(draft)).toBe(false);
		presets.write(draft, {});
		expect(restore.disabled?.(draft)).toBe(true);
	});

	it('switches booleans and two-value settings', () => {
		const draft = toDraft(STORED);
		const zoom = row('smoothZoom');
		if (zoom.kind !== 'switch') throw new Error('smoothZoom is not a switch');
		zoom.write(draft, false);
		expect(draft.smoothZoom).toBe(false);

		const visibility = row('toolbarVisibility');
		if (visibility.kind !== 'switch') throw new Error('toolbarVisibility is not a switch');
		expect(visibility.read(draft)).toBe(false);
		visibility.write(draft, true);
		expect(draft.toolbarVisibility).toBe('onHover');
		visibility.write(draft, false);
		expect(draft.toolbarVisibility).toBe('always');
	});

	it('keeps the floating options while the toolbar is docked, and says what they are for', () => {
		// Full screen floats the toolbar whatever its placement (section 6.1).
		const draft = toDraft({ ...STORED, toolbarStyle: 'panel' });
		const says = (id: string, d: typeof draft) => {
			const { description } = row(id);
			return typeof description === 'function' ? description(d) : (description ?? '');
		};
		for (const id of ['toolbarPosition', 'toolbarVisibility']) {
			expect(row(id).disabled?.(draft) ?? false, id).toBe(false);
			expect(says(id, draft), id).toMatch(/full screen/);
			expect(says(id, toDraft(STORED)), id).not.toMatch(/full screen/);
		}
	});

	it('offers where to dock the toolbar only while it is docked', () => {
		const dock = row('toolbarDock');
		if (dock.kind !== 'choice') throw new Error('toolbarDock is not a dropdown');
		expect(dock.options.map((o) => o.value)).toEqual(['top', 'left', 'right']);
		expect(dock.disabled?.(toDraft(STORED))).toBe(true);
		const draft = toDraft({ ...STORED, toolbarStyle: 'panel' });
		expect(dock.disabled?.(draft)).toBe(false);
		dock.write(draft, 'right');
		expect(toInput(draft).toolbarDock).toBe('right');
		// It floats at any edge of the pages.
		const position = row('toolbarPosition');
		if (position.kind !== 'choice') throw new Error('toolbarPosition is not a dropdown');
		expect(position.options.map((o) => o.value)).toEqual(['bottom', 'top', 'left', 'right']);
	});

	it('keeps quick tools in the bar’s order whatever order they are switched on', () => {
		const draft = toDraft(STORED);
		for (const id of ['quickTools-bookmark', 'quickTools-underline']) {
			const r = row(id);
			if (r.kind === 'switch') r.write(draft, true);
		}
		const copy = row('quickTools-copy');
		if (copy.kind === 'switch') copy.write(draft, false);
		expect(draft.quickTools).toEqual(['highlight', 'underline', 'bookmark']);
	});

	it('offers to restore the defaults with a button, not a value', () => {
		const reset = row(RESET_ROW);
		expect(reset.kind).toBe('action');
		expect('key' in reset).toBe(false);
	});

	it('sends the shortcuts as its own copy', () => {
		const draft = toDraft(STORED);
		const input = toInput(draft);
		expect(input.shortcuts).toEqual({ save: ['Ctrl+Alt+S'], copy: [] });
		expect(input.shortcuts).not.toBe(draft.shortcuts);
	});

	it('ignores a choice that is not offered', () => {
		const draft = toDraft(STORED);
		const theme = row('appearance');
		if (theme.kind !== 'segments') throw new Error('appearance is not a pill of segments');
		expect(theme.options.map((o) => o.value)).toEqual(['system', 'light', 'dark']);
		theme.write(draft, 'sepia');
		expect(draft.appearance).toBe('system');
		theme.write(draft, 'dark');
		expect(draft.appearance).toBe('dark');
	});
});
