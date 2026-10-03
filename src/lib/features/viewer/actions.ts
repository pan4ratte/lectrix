// Viewer actions used by menus, shortcuts and the context menu.

import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { ordered, selectionText } from './selection.ts';

/** Copies the selected text to the clipboard (honoring the document's copy permission). */
export async function copySelection(tab: DocTab): Promise<void> {
	const sel = tab.selection;
	if (!sel) return;
	if (!tab.flags.canCopy) {
		app.notify({
			kind: 'error',
			message: 'This document’s security settings don’t allow copying text.',
			suggestion: 'Ask the document’s author for an unrestricted copy.'
		});
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
