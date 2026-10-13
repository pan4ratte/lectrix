// Starting and leaving the page organizing tool (section 6.7), for the command, the view
// bar's button and the grid.

import { tools } from '#lib/features/annotations/state.svelte.ts';
import { fullscreen } from '#lib/features/fullscreen/fullscreen.svelte.ts';
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

export function toggleOrganize(tab: DocTab) {
	if (tab.organize) leaveOrganize(tab);
	else void organizePages(tab);
}
