// Pages exported as images (section 6.14): File > Export > Pages as images…, a page's
// right-click menu in the viewer, and Organize pages' bar and menu open the Export as
// images dialog for the document shown.

import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

/**
 * Opens the dialog for `tab`: for all its pages, or for `page` alone (a page right-clicked),
 * or for `selected` (the pages selected in Organize pages).
 */
export function openExport(tab: DocTab, from: { page?: number; selected?: readonly number[] } = {}) {
	const selected = from.selected && from.selected.length > 0 ? [...from.selected].sort((a, b) => a - b) : null;
	app.exportRequest = {
		tabId: tab.id,
		page: from.page ?? null,
		selected
	};
}
