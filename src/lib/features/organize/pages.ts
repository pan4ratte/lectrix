// Organizing a document's pages (section 6.7): what the grid says about a page, where a
// nudge puts the selected pages, and the words for deleting. Pure functions, so the state
// and its tests share them.

import { pageName } from '#lib/features/viewer/pagebox.ts';
import type { DeleteReport, MergeReport } from '#lib/ipc/index.ts';

/**
 * Under a page's picture: its label, with its place in the document after it where that
 * says something else ("iv (4)"), since labels stay with the places while pages move. A
 * page whose label is empty shows its place alone.
 */
export function caption(index: number, labels: readonly string[] | null): string {
	const number = String(index + 1);
	const label = labels?.[index];
	if (label === undefined || label === number) return number;
	return label === '' ? `(${number})` : `${label} (${number})`;
}

/** A page's accessible name: "4 of 312: page iv". */
export function describePage(index: number, count: number, labels: readonly string[] | null): string {
	return `${index + 1} of ${count}: page ${pageName(index, labels)}`;
}

/**
 * The gap that moves the pages at `indexes` (sorted) one place earlier (`-1`) or later
 * (`1`), as a block, or null if they are at that end already.
 */
export function nudgeGap(indexes: readonly number[], count: number, direction: 1 | -1): number | null {
	const first = indexes[0];
	const last = indexes[indexes.length - 1];
	if (first === undefined || last === undefined) return null;
	if (direction < 0) return first === 0 ? null : first - 1;
	// The gap just past the page after the last one.
	return last === count - 1 ? null : last + 2;
}

function count(n: number, one: string, many: string) {
	return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

/**
 * Where pages inserted before page `at` (0-based; the page count appends) go, in words:
 * "after page iv", naming the page as the page box does, or "before the first page".
 */
export function insertPlace(at: number, pageCount: number, labels: readonly string[] | null): string {
	if (at <= 0) return 'before the first page';
	if (at >= pageCount) return 'after the last page';
	return `after page ${pageName(at - 1, labels)}`;
}

/**
 * What is said once pages are extracted into a new document: how many, that it is not a
 * file yet, and what could not go with them (links and bookmarks that led to pages left
 * behind).
 */
export function extractedMessage(report: MergeReport) {
	const dropped: string[] = [];
	if (report.droppedLinks) dropped.push(count(report.droppedLinks, 'link', 'links'));
	if (report.droppedBookmarks) dropped.push(count(report.droppedBookmarks, 'bookmark', 'bookmarks'));
	const single = report.droppedLinks + report.droppedBookmarks === 1;
	const parts = ['It is not saved yet: save it to keep it.'];
	if (dropped.length) {
		parts.push(`${dropped.join(' and ')} that led to pages left behind ${single ? 'was' : 'were'} not copied.`);
	}
	return {
		message: `Extracted ${count(report.pages, 'page', 'pages')} into a new document.`,
		suggestion: parts.join(' ')
	};
}

/** The question asked before pages are deleted. */
export function deleteQuestion(indexes: readonly number[], labels: readonly string[] | null) {
	const one = indexes.length === 1;
	return {
		title: one ? `Delete page ${pageName(indexes[0]!, labels)}?` : `Delete ${indexes.length.toLocaleString()} pages?`,
		message: `Links and bookmarks that lead to ${one ? 'it' : 'them'} are deleted with ${one ? 'it' : 'them'}.`,
		detail: 'You can undo this until you save the document.',
		buttons: [
			{ id: 'delete', label: 'Delete', danger: true },
			{ id: 'cancel', label: 'Cancel' }
		],
		cancel: 'cancel'
	};
}

/**
 * What is said once pages are deleted: how many, what went with them, and that they stay
 * in the file until it is saved with Save As (optimized), since an ordinary save adds to
 * the file and keeps what was there (section 5.4).
 */
export function deletedMessage(report: DeleteReport) {
	const one = report.pages === 1;
	const them = one ? 'it' : 'them';
	const parts: string[] = [];
	const leading: string[] = [];
	if (report.links) leading.push(count(report.links, 'link', 'links'));
	if (report.bookmarks) leading.push(count(report.bookmarks, 'bookmark', 'bookmarks'));
	if (leading.length) {
		const single = report.links + report.bookmarks === 1;
		parts.push(`${leading.join(' and ')} that led to ${them} ${single ? 'was' : 'were'} deleted too.`);
	}
	if (report.fields) {
		parts.push(`${count(report.fields, 'form field', 'form fields')} on ${them} ${report.fields === 1 ? 'was' : 'were'} deleted too.`);
	}
	parts.push(
		`${one ? 'The page stays' : 'The pages stay'} inside the file, where ${one ? 'it' : 'they'} can be recovered, until you save with Save As (optimized).`
	);
	return {
		message: `Deleted ${count(report.pages, 'page', 'pages')}.`,
		suggestion: parts.join(' ')
	};
}
