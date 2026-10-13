// Starting and leaving the page organizing tool (section 6.7), for the command, the view
// bar's button and the grid.

import { tools } from '#lib/features/annotations/state.svelte.ts';
import { fullscreen } from '#lib/features/fullscreen/fullscreen.svelte.ts';
import type { OpenResult } from '#lib/ipc/index.ts';
import { isTextInput } from '#lib/shortcuts.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { OrganizeState } from './organize.svelte.ts';

/** Shows the document's pages as a grid to organize, in place of the viewer. */
export async function organizePages(tab: DocTab) {
	if (tab.organize) return;
	// A comment or a name being typed is written when its field is left.
	const field = document.activeElement;
	if (field instanceof HTMLElement && isTextInput(field)) field.blur();
	// Full screen shows the pages alone; the grid has a bar of its own.
	if (fullscreen.on) await fullscreen.leave().catch(() => {});
	tools.tool = 'select';
	tab.draft = null;
	tab.selection = null;
	tab.selectedAnnotation = null;
	if (tab.search.open) {
		tab.search.open = false;
		tab.search.clear();
	}
	app.rememberView(tab);
	tab.organize = new OrganizeState(tab);
}

/** Back to reading: where the document was, or at the top of `page`. */
export function leaveOrganize(tab: DocTab, page?: number) {
	if (!tab.organize) return;
	tab.organize.stopZoom();
	tab.organize = null;
	if (page !== undefined) {
		tab.currentPage = page;
		tab.pendingPosition = { page, offset: 0 };
	}
}

/**
 * Insert pages from files (section 6.4): asks for the files, then for the options. The
 * pages go into the grid of pages, which is shown for it if it is not yet (and left again
 * if the insertion is given up): before page `at`, or else after the selected pages, or
 * the page the document is at.
 */
export async function insertFromFile(tab: DocTab, at?: number) {
	if (app.inserting || !app.canInsert(tab)) return;
	const started = !tab.organize;
	if (started) {
		await organizePages(tab);
		if (at !== undefined && at > 0) tab.organize?.show(at - 1, false);
	}
	const organize = tab.organize;
	if (!organize) return;
	await app.insertFromFile(tab, at ?? organize.insertGap(), started ? () => leaveOrganize(tab) : undefined);
}

/** Files dropped on the grid of pages were opened: their pages are inserted where they
 * were dropped, after the options are asked. */
export function insertDropped(tab: DocTab, results: OpenResult[]) {
	app.insertOpened(tab, results, tab.organize?.takeDrop() ?? tab.currentPage + 1);
}

export function toggleOrganize(tab: DocTab) {
	if (tab.organize) leaveOrganize(tab);
	else void organizePages(tab);
}

/** The start screen's Organize pages: asks for a file and shows its pages as a grid to
 * organize. A file that is open already is organized in its tab. */
export async function organizeFile() {
	const tab = await app.open();
	if (tab) await organizePages(tab);
}
