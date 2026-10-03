// Bookmark actions shared by the panel, the inspector, the menus and the shortcuts
// (section 6.2). Each change is one operation in Rust, so it is one undo step.

import { selectedText } from '#lib/features/viewer/actions.ts';
import { logError, setBookmarkOpen, type Bookmark } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import {
	descendantCount,
	insertionPoint,
	keyboardMove,
	locate,
	selectionAfterDelete,
	titleFromText,
	type KeyboardMove,
	type Position
} from './tree.ts';

/** Tells the user why bookmarks can't be changed, if they can't. */
function refuseEdit(tab: DocTab): boolean {
	if (tab.outline.damaged) {
		app.notify({
			kind: 'error',
			message: 'This document’s bookmarks are damaged, so Folio can show them but not change them.'
		});
		return true;
	}
	if (!tab.flags.canAssemble) {
		app.notify({
			kind: 'error',
			message: 'This document’s security settings don’t allow changing bookmarks.',
			suggestion: 'Ask the document’s author for an unrestricted copy.'
		});
		return true;
	}
	return false;
}

/** Expands or collapses a bookmark. Saved with the document, but not an edit. */
export function setOpen(tab: DocTab, bookmark: Bookmark, open: boolean) {
	if (bookmark.open === open || bookmark.children.length === 0) return;
	bookmark.open = open;
	setBookmarkOpen(tab.id, bookmark.id, open).catch((e: unknown) => {
		void logError(`set_bookmark_open failed: ${String(e)}`);
	});
}

/** Expands the ancestors of a bookmark so it is visible. */
export function revealBookmark(tab: DocTab, id: number) {
	const loc = locate(tab.outline.items, id);
	for (const ancestor of loc?.ancestors ?? []) setOpen(tab, ancestor, true);
}

function showBookmarks() {
	app.sidebarOpen = true;
	app.sidebarPanel = 'bookmarks';
}

/**
 * Ctrl+B: a bookmark for the current view (top-left of what is visible). Selected text
 * becomes its title; without it, the bookmark starts with the page label and opens for
 * renaming.
 */
export async function addBookmark(tab: DocTab) {
	if (refuseEdit(tab) || !tab.viewer) return;
	const at = tab.viewer.topLeft();
	// The copy permission also covers taking text out of the document this way.
	const text = tab.flags.canCopy && tab.selection ? titleFromText(await selectedText(tab)) : '';
	const label = tab.labels?.[at.page] ?? String(at.page + 1);
	const position = insertionPoint(tab.outline.items, tab.selectedBookmark, at);
	if (position.parent !== null) revealBookmark(tab, position.parent);
	showBookmarks();
	const change = await app.apply(tab, {
		kind: 'addBookmark',
		parent: position.parent,
		index: position.index,
		title: text || `Page ${label}`,
		dest: at
	});
	if (change?.created != null) {
		tab.selectedBookmark = change.created;
		if (!text) tab.renamingBookmark = change.created;
	}
}

/** Commits an inline rename. Empty or unchanged titles change nothing. */
export async function renameBookmark(tab: DocTab, id: number, title: string) {
	tab.renamingBookmark = null;
	const current = locate(tab.outline.items, id)?.bookmark;
	const cleaned = title.replace(/\s+/g, ' ').trim();
	if (!current || !cleaned || cleaned === current.title) return;
	if (refuseEdit(tab)) return;
	await app.apply(tab, { kind: 'renameBookmark', id, title: cleaned });
}

export function startRename(tab: DocTab, id: number) {
	if (refuseEdit(tab)) return;
	tab.selectedBookmark = id;
	tab.renamingBookmark = id;
}

/** Deletes a bookmark; one with children only after confirmation (section 6.2). */
export async function deleteBookmark(tab: DocTab, id: number) {
	const loc = locate(tab.outline.items, id);
	if (!loc || refuseEdit(tab)) return;
	const inside = descendantCount(loc.bookmark);
	if (inside > 0) {
		const choice = await app.ask({
			title: 'Delete bookmark?',
			message: `Delete “${loc.bookmark.title}” and the ${inside === 1 ? 'bookmark' : `${inside} bookmarks`} inside it?`,
			detail: 'You can undo this with Ctrl+Z.',
			buttons: [
				{ id: 'delete', label: 'Delete', primary: true },
				{ id: 'cancel', label: 'Cancel' }
			],
			cancel: 'cancel'
		});
		if (choice !== 'delete') return;
	}
	const next = selectionAfterDelete(tab.outline.items, id);
	const change = await app.apply(tab, { kind: 'deleteBookmark', id });
	if (change) tab.selectedBookmark = next;
}

/** Points a bookmark at the current view. */
export async function setDestinationHere(tab: DocTab, id: number) {
	if (refuseEdit(tab) || !tab.viewer) return;
	await app.apply(tab, { kind: 'setBookmarkDestination', id, dest: tab.viewer.topLeft() });
}

export async function moveBookmark(tab: DocTab, id: number, to: Position) {
	if (refuseEdit(tab)) return;
	const change = await app.apply(tab, { kind: 'moveBookmark', id, parent: to.parent, index: to.index });
	if (change) {
		tab.selectedBookmark = id;
		revealBookmark(tab, id);
	}
}

export async function keyboardMoveBookmark(tab: DocTab, id: number, move: KeyboardMove) {
	const to = keyboardMove(tab.outline.items, id, move);
	if (to) await moveBookmark(tab, id, to);
}

/** Follows a bookmark. Only places in this document are followed; for the rest, the user
 * is told what the bookmark does (Folio stays offline and runs no actions). */
export function goToBookmark(tab: DocTab, id: number) {
	const bookmark = locate(tab.outline.items, id)?.bookmark;
	if (!bookmark) return;
	const target = bookmark.target;
	switch (target.kind) {
		case 'page':
			tab.viewer?.goToPoint(target.page, target.x, target.y);
			return;
		case 'uri':
			app.notify({
				kind: 'info',
				message: `This bookmark is a web link: ${target.uri}`,
				suggestion: 'Folio doesn’t open links. Copy it to open it in your browser.',
				action: { label: 'Copy link', run: () => void copyLink(target.uri) }
			});
			return;
		case 'broken':
			app.notify({
				kind: 'error',
				message: 'This bookmark’s destination doesn’t exist in this document.',
				suggestion: 'Use “Set destination to current view” to point it somewhere.'
			});
			return;
		case 'file':
			app.notify({
				kind: 'info',
				message: `This bookmark opens another file${target.file ? `: ${target.file}` : ''}.`,
				suggestion: 'Folio doesn’t follow links to other files.'
			});
			return;
		case 'action':
			app.notify({
				kind: 'info',
				message: `This bookmark runs a ${target.action || 'custom'} action, which Folio doesn’t do.`
			});
			return;
		case 'none':
			return;
	}
}

export async function copyLink(uri: string) {
	try {
		await navigator.clipboard.writeText(uri);
		app.notify({ kind: 'info', message: 'Link copied.' }, 3000);
	} catch {
		app.notify({ kind: 'error', message: 'The link couldn’t be copied to the clipboard.', suggestion: 'Try again.' });
	}
}
