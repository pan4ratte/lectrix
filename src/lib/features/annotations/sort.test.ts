import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { AnnotationSort, Settings, SettingsInput } from '#lib/ipc/index.ts';

const store = vi.hoisted(() => ({ settings: null as Settings | null, showError: vi.fn() }));
const ipc = vi.hoisted(() => ({ setSettings: vi.fn() }));
vi.mock('#lib/stores/app.svelte.ts', () => ({ app: store }));
vi.mock('#lib/features/settings/apply.ts', () => ({ applySettings: (s: Settings) => (store.settings = s) }));
vi.mock('#lib/features/settings/schema.ts', () => ({
	toDraft: (s: Settings) => s,
	toInput: (d: Settings) => d
}));
vi.mock('#lib/ipc/index.ts', () => ({ setSettings: ipc.setSettings, toAppError: (e: unknown) => e }));

const { DEFAULT_SORT, ownSort, pickSort, shownSort } = await import('./sort.ts');

const byAuthor: AnnotationSort = { sort: 'author', order: 'desc' };

function settings(remember: boolean, annotationSort = DEFAULT_SORT): Settings {
	return { rememberAnnotationSort: remember, annotationSort } as Settings;
}

describe('the annotation list’s sort', () => {
	beforeEach(() => {
		ipc.setSettings.mockReset();
		ipc.setSettings.mockImplementation(async (input: SettingsInput) => input);
	});

	it('is the one Settings keep, in every document, while they keep it', async () => {
		store.settings = settings(true);
		const [a, b] = [{}, {}];
		await pickSort(a, byAuthor);
		expect(ipc.setSettings).toHaveBeenCalledWith(expect.objectContaining({ annotationSort: byAuthor }));
		expect(shownSort(store.settings, ownSort(b))).toEqual(byAuthor);
		// The same sort again stores nothing.
		await pickSort(b, byAuthor);
		expect(ipc.setSettings).toHaveBeenCalledTimes(1);
	});

	it('is each document’s own when Settings don’t keep it, from page A–Z', async () => {
		store.settings = settings(false);
		const [a, b] = [{}, {}];
		await pickSort(a, byAuthor);
		expect(ipc.setSettings).not.toHaveBeenCalled();
		expect(shownSort(store.settings, ownSort(a))).toEqual(byAuthor);
		expect(shownSort(store.settings, ownSort(b))).toEqual(DEFAULT_SORT);
		expect(shownSort(null, ownSort(b))).toEqual(DEFAULT_SORT);
	});
});
