// Pages exported as images (section 6.14): File > Export > Pages as images…, a page's
// right-click menu in the viewer, and Organize pages' bar and menu open the Export as
// images dialog for the document shown. Copy page as image puts one page on the clipboard.

import { pageName } from '#lib/features/viewer/pagebox.ts';
import { copyPageImage, toAppError } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';
import { copiedMessage } from './export.ts';

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

/** The page Edit > Copy page as image copies: the current page, or in Organize pages the one
 * selected page (none with several, as the clipboard holds one image). */
export function pageToCopy(tab: DocTab): number | null {
	if (!tab.organize) return tab.currentPage;
	const picked = tab.organize.indexes(tab.organize.selected);
	return picked.length === 1 ? picked[0]! : null;
}

let copying = false;

/** Puts page `page` of `tab` on the clipboard as an image, and says so. */
export async function copyPage(tab: DocTab, page: number | null = pageToCopy(tab)) {
	if (page === null || copying) return;
	copying = true;
	try {
		const copied = await copyPageImage(tab.id, page);
		const asked = app.settings?.copyImageDpi ?? copied.dpi;
		app.notify({ kind: 'info', message: copiedMessage(pageName(page, tab.displayLabels), copied.dpi, asked) });
	} catch (e) {
		app.showError(toAppError(e));
	} finally {
		copying = false;
	}
}
