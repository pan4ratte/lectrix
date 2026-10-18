// Find in document (section 6.1): the commands that open the search bar and step through
// its matches.

import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab, SearchFrom } from '#lib/stores/doc.svelte.ts';

import { selectedText } from './actions.ts';
import { ordered } from './selection.ts';

/** The most characters of selected text the search field takes. */
export const MAX_SEED_CHARS = 200;

/** Whether going to a match glides there (Settings; reduced motion stills it anyway). */
export function searchGlides(): boolean {
	return app.settings?.smoothSearchScroll !== false;
}

/**
 * Find (Ctrl+F): opens the search bar, or puts the keyboard focus back in its field. With
 * text selected, the field takes that text and the search starts at the selection itself,
 * unless the document forbids copying its text (the field's text could be copied); without,
 * the text the field kept is searched again.
 */
export async function find(tab: DocTab) {
	const seed = await selectionSeed(tab);
	if (seed && (seed.text !== tab.search.query || tab.search.hits.length === 0)) {
		tab.search.input = seed.text;
		void tab.search.start(seed.text, seed.from, { go: 'reveal', smooth: searchGlides() });
	} else {
		// The text the field kept when the bar closed: searched again, the view left where it is.
		tab.search.resume();
	}
	if (!tab.search.open) {
		// The bar puts the focus in its field as it opens.
		tab.search.open = true;
		return;
	}
	const input = document.querySelector<HTMLInputElement>('[data-search-input]');
	input?.focus();
	input?.select();
}

/** Goes to the next or previous match. */
export function findStep(tab: DocTab, direction: 1 | -1) {
	tab.search.step(direction, { smooth: searchGlides() });
}

/** What a search for the selected text starts from: the text on one line, and the middle
 * of its first character. */
async function selectionSeed(tab: DocTab): Promise<{ text: string; from: SearchFrom } | null> {
	const sel = tab.selection;
	if (!sel || !tab.flags.canCopy) return null;
	const text = (await selectedText(tab)).replace(/\s+/g, ' ').trim().slice(0, MAX_SEED_CHARS).trim();
	if (!text) return null;
	const [start] = ordered(sel.anchor, sel.focus);
	const line = tab.text(start.page)?.lines[start.line];
	let offset = start.offset;
	while (line && offset < line.chars.length && /^\s$/.test(line.chars[offset]!)) offset++;
	if (!line || offset >= line.chars.length) return { text, from: { page: start.page } };
	const b = line.boxes;
	const i = offset * 4;
	return { text, from: { page: start.page, point: [(b[i]! + b[i + 2]!) / 2, (b[i + 1]! + b[i + 3]!) / 2] } };
}
