// Links on a page, and following them (section 6.1); bookmarks are followed the same way
// (section 6.2). A place in this document is gone to, and a web address opens in the
// system's browser. Another file, an action or an address of another kind is described in
// a notification: Lectrix opens no files and runs no actions.

import { openLink, toAppError, type BookmarkTarget, type LinkPlace, type PageLink } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

/** The link at a point of its page (page points); of links that overlap, the one on top. */
export function linkAt(links: readonly PageLink[], x: number, y: number): PageLink | null {
	for (let i = links.length - 1; i >= 0; i--) {
		const [x0, y0, x1, y1] = links[i]!.rect;
		if (x >= x0 && x <= x1 && y >= y0 && y <= y1) return links[i]!;
	}
	return null;
}

/** What is said of a target that is not followed: `message`, and what to do about it. */
export interface TargetNotice {
	kind: 'info' | 'error';
	message: string;
	suggestion?: string;
	/** A web address to offer for copying. */
	uri?: string;
}

/**
 * What to tell the user about the target of `what` ("bookmark" or "link") when it is not
 * a place in this document; null when it is one, or when it does nothing.
 */
export function targetNotice(target: BookmarkTarget, what: 'bookmark' | 'link'): TargetNotice | null {
	switch (target.kind) {
		case 'page':
		case 'none':
			return null;
		case 'uri':
			// A web address opens in the browser (`follow`); what gets here is an address of
			// another kind (mail, a file).
			return {
				kind: 'info',
				message: `This ${what} leads to: ${target.uri}`,
				suggestion: 'Lectrix opens web addresses only. Copy it to use it elsewhere.',
				uri: target.uri
			};
		case 'broken':
			return {
				kind: 'error',
				message: `This ${what}’s destination doesn’t exist in this document.`,
				suggestion: what === 'bookmark' ? 'Use “Set destination to current view” to point it somewhere.' : undefined
			};
		case 'file':
			return {
				kind: 'info',
				message: `This ${what} opens another file${target.file ? `: ${target.file}` : ''}.`,
				suggestion: 'Lectrix doesn’t follow links to other files.'
			};
		case 'action':
			return {
				kind: 'info',
				message: `This ${what} runs a ${target.action || 'custom'} action, which Lectrix doesn’t do.`
			};
	}
}

/** A web address Lectrix opens in the browser: http or https. (Rust decides; this only
 * saves asking it about the rest.) */
export function isWebAddress(uri: string): boolean {
	return /^https?:\/\/\S+$/i.test(uri);
}

/**
 * Follows the target of a bookmark or a link that was clicked: goes to a place in this
 * document (recorded in back/forward history), opens a web address in the system's
 * browser, saying which address that was, and says what anything else is. `from` is where
 * the document has the target: Rust opens only an address it finds there.
 */
export function follow(tab: DocTab, target: BookmarkTarget, from: LinkPlace) {
	if (target.kind === 'page') {
		tab.viewer?.goToPoint(target.page, target.x, target.y);
		return;
	}
	if (target.kind === 'uri' && isWebAddress(target.uri)) {
		void openInBrowser(tab, from, target.uri);
		return;
	}
	const notice = targetNotice(target, from.kind === 'bookmark' ? 'bookmark' : 'link');
	if (!notice) return;
	const uri = notice.uri;
	app.notify({
		kind: notice.kind,
		message: notice.message,
		suggestion: notice.suggestion,
		action: uri === undefined ? undefined : { label: 'Copy link', run: () => void copyLink(uri) }
	});
}

async function openInBrowser(tab: DocTab, from: LinkPlace, uri: string) {
	try {
		await openLink(tab.id, from, uri);
		app.notify({ kind: 'info', message: 'Opened in your browser', suggestion: uri }, 5000);
	} catch (e) {
		app.showError(toAppError(e), { label: 'Copy link', run: () => void copyLink(uri) });
	}
}

/** The longest address a preview shows whole. */
const PREVIEW_ADDRESS = 160;

/**
 * Where a link leads, for the tooltip on it (section 6.1): the page as the page box names
 * it (its label, `labels`, else its number), a web or other address, or what else it does.
 * Empty for a link that does nothing.
 */
export function linkPreview(target: BookmarkTarget, labels: readonly string[] | null): string {
	switch (target.kind) {
		case 'page':
			return `Page ${labels?.[target.page] || target.page + 1}`;
		case 'uri':
			return target.uri.length > PREVIEW_ADDRESS ? `${target.uri.slice(0, PREVIEW_ADDRESS)}…` : target.uri;
		case 'file':
			return target.file ? `Another file: ${target.file}` : 'Another file';
		case 'action':
			return `${target.action || 'Custom'} action, which Lectrix doesn’t run`;
		case 'broken':
			return 'Leads nowhere in this document';
		case 'none':
			return '';
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
