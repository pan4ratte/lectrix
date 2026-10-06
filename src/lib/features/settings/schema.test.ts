import { describe, expect, it } from 'vitest';

import type { Settings } from '#lib/ipc/index.ts';

import { SETTING_GROUPS, toDraft, toInput, type SettingRow } from './schema.ts';

/** Every stored setting, as Rust sends them (a new field in Rust fails to compile here). */
const STORED: Settings = {
	author: 'Ada',
	defaultAuthor: 'ada',
	appearance: 'system',
	toolbarStyle: 'floating',
	toolbarPosition: 'bottom',
	toolbarVisibility: 'always',
	quickTools: ['highlight', 'copy'],
	checkForUpdates: true,
	smoothZoom: true,
	smoothAnnotationScroll: true,
	tooltipDelayMs: 300,
	openCommentAfterMarkup: false,
	rememberAnnotationStyle: true
};

const rows = SETTING_GROUPS.flatMap((g) => g.sections.flatMap((s) => s.rows));
const row = (id: string): SettingRow => {
	const found = rows.find((r) => r.id === id);
	if (!found) throw new Error(`no row ${id}`);
	return found;
};

describe('settings schema', () => {
	it('has a row for every setting, and unique ids', () => {
		const shown = new Set(rows.map((r) => r.key));
		for (const key of Object.keys(toInput(toDraft(STORED)))) expect(shown, key).toContain(key);
		expect(new Set(rows.map((r) => r.id)).size).toBe(rows.length);
		expect(new Set(SETTING_GROUPS.map((g) => g.id)).size).toBe(SETTING_GROUPS.length);
	});

	it('has no empty group or section', () => {
		for (const g of SETTING_GROUPS) {
			expect(g.sections.length, g.id).toBeGreaterThan(0);
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

	it('disables the floating options while the toolbar is docked', () => {
		const draft = toDraft({ ...STORED, toolbarStyle: 'panel' });
		expect(row('toolbarPosition').disabled?.(draft)).toBe(true);
		expect(row('toolbarVisibility').disabled?.(draft)).toBe(true);
		expect(row('toolbarPosition').disabled?.(toDraft(STORED))).toBe(false);
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

	it('ignores a choice that is not offered', () => {
		const draft = toDraft(STORED);
		const theme = row('appearance');
		if (theme.kind !== 'choice') throw new Error('appearance is not a choice');
		theme.write(draft, 'sepia');
		expect(draft.appearance).toBe('system');
		theme.write(draft, 'dark');
		expect(draft.appearance).toBe('dark');
	});
});
