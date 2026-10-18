// The annotation list's sort (section 6.5). While Settings keep it ("Keep the annotation
// list's sort for every document"), it is the one last picked, stored with the settings: in
// every document and window, and after a restart. Otherwise each document has its own,
// from page A–Z, kept while it is open.

import { applySettings } from '#lib/features/settings/apply.ts';
import { toDraft, toInput } from '#lib/features/settings/schema.ts';
import { setSettings, toAppError, type AnnotationSort, type Settings } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';

export const DEFAULT_SORT: AnnotationSort = { sort: 'page', order: 'asc' };

/** Each document's own sort, for when Settings don't keep one. */
const own = new WeakMap<object, AnnotationSort>();

const same = (a: AnnotationSort, b: AnnotationSort) => a.sort === b.sort && a.order === b.order;

/** What `doc`'s list starts with when Settings don't keep a sort. */
export function ownSort(doc: object): AnnotationSort {
	return own.get(doc) ?? DEFAULT_SORT;
}

/** What a list shows: the sort Settings keep, or its document's own. */
export function shownSort(settings: Settings | null, docSort: AnnotationSort): AnnotationSort {
	return settings?.rememberAnnotationSort ? settings.annotationSort : docSort;
}

/** Notes a sort picked in `doc`'s list, and stores it with the settings while they keep it. */
export async function pickSort(doc: object, choice: AnnotationSort) {
	own.set(doc, choice);
	const s = app.settings;
	if (!s?.rememberAnnotationSort || same(s.annotationSort, choice)) return;
	// Shown at once; stored (and sent to the other windows) on the way.
	app.settings = { ...s, annotationSort: choice };
	try {
		applySettings(await setSettings(toInput({ ...toDraft(s), annotationSort: choice })));
	} catch (e) {
		app.showError(toAppError(e));
	}
}
