// Viewer actions used by menus, shortcuts and the context menu.

import { refuseRestricted } from '#lib/features/unprotect/actions.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { ordered, selectionText } from './selection.ts';

/** Copies the selected text to the clipboard (honoring the document's copy permission). */
export async function copySelection(tab: DocTab): Promise<void> {
	const sel = tab.selection;
	if (!sel) return;
	if (!tab.flags.canCopy) {
		refuseRestricted(tab, 'This document’s security settings don’t allow copying text.');
		return;
	}
	const text = await selectedText(tab);
	if (!text) return;
	try {
		await navigator.clipboard.writeText(text);
	} catch {
		app.notify({
			kind: 'error',
			message: 'The text couldn’t be copied to the clipboard.',
			suggestion: 'Try again.'
		});
	}
}

/**
 * Turns pages by `degrees` as they are shown (section 6.1, view rotation), on top of the
 * view rotation. On screen only: nothing to save or undo, and a file open for reading only
 * has it too. Remembered for the file with the rest of its view.
 */
export function turnPages(tab: DocTab, pages: readonly number[], degrees: number) {
	if (tab.viewer) tab.viewer.turnPages(pages, degrees);
	else tab.turnPages(pages, degrees);
}

/** The selected text ('' without a selection). Does not check the copy permission. */
export async function selectedText(tab: DocTab): Promise<string> {
	const sel = tab.selection;
	if (!sel) return '';
	const [start, end] = ordered(sel.anchor, sel.focus);
	// Pages between the ends of a long selection may not have their text loaded yet.
	const loads: Promise<unknown>[] = [];
	for (let page = start.page; page <= end.page; page++) {
		if (!tab.text(page)) loads.push(tab.loadText(page));
	}
	await Promise.all(loads);
	return selectionText(tab.textMap(), start, end);
}
