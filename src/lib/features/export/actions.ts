// Pages exported as images (section 6.14): File > Export > Pages as images…, a page's
// right-click menu in the viewer, and Organize pages' bar and menu open the Export as
// images dialog for the document shown. Copy page as image puts one page on the clipboard.
// The Snapshot tool's area is copied or saved alone, the same way.

import { pageName } from '#lib/features/viewer/pagebox.ts';
import { copyPageImage, toAppError, type ExportArea } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';
import { areaCopiedMessage, copiedMessage } from './export.ts';

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

/** The area the Snapshot tool framed in `tab`, as Rust takes it: turned as its page is shown,
 * so the image looks as it was framed. */
export function snapshotArea(tab: DocTab): { page: number; area: ExportArea } | null {
	const shot = tab.snapshot;
	if (!shot) return null;
	const [x0, y0, x1, y1] = shot.box;
	return { page: shot.page, area: { x0, y0, x1, y1, turn: tab.shownRotation(shot.page) } };
}

/** Puts the Snapshot tool's area on the clipboard as an image, and says so. */
export async function copyArea(tab: DocTab) {
	const found = snapshotArea(tab);
	if (!found || copying) return;
	copying = true;
	try {
		const copied = await copyPageImage(tab.id, found.page, found.area);
		const asked = app.settings?.copyImageDpi ?? copied.dpi;
		app.notify({ kind: 'info', message: areaCopiedMessage(copied.dpi, asked) });
	} catch (e) {
		app.showError(toAppError(e));
	} finally {
		copying = false;
	}
}

/** Opens the Export as images dialog for the Snapshot tool's area. */
export function saveArea(tab: DocTab) {
	const found = snapshotArea(tab);
	if (!found) return;
	app.exportRequest = { tabId: tab.id, page: found.page, selected: null, area: found.area };
}
