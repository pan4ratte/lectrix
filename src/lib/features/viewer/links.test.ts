import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { BookmarkTarget, LinkPlace, PageLink } from '#lib/ipc/index.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

const store = vi.hoisted(() => ({
	notify: vi.fn(),
	showError: vi.fn(),
	settings: null as { smoothBookmarkScroll: boolean } | null
}));
const ipc = vi.hoisted(() => ({ openLink: vi.fn<(id: number, from: LinkPlace, uri: string) => Promise<void>>(async () => {}) }));
vi.mock('#lib/stores/app.svelte.ts', () => ({ app: store }));
vi.mock('#lib/ipc/index.ts', () => ({ ...ipc, toAppError: (e: unknown) => e }));

const { follow, isWebAddress, linkAt, linkPreview, targetNotice } = await import('./links.ts');

const page = (n: number): BookmarkTarget => ({ kind: 'page', page: n, x: null, y: null, named: null });
const link = (rect: [number, number, number, number], target: BookmarkTarget): PageLink => ({ rect, target });
const ON_PAGE: LinkPlace = { kind: 'page', page: 4 };
const BOOKMARK: LinkPlace = { kind: 'bookmark', bookmark: 12 };

beforeEach(() => vi.clearAllMocks());

describe('the link under the pointer', () => {
	const links = [link([60, 100, 380, 126], page(14)), link([60, 140, 380, 166], page(30)), link([300, 90, 400, 150], page(58))];

	it('is the one whose area holds the point', () => {
		expect(linkAt(links, 100, 110)?.target).toEqual(page(14));
		expect(linkAt(links, 100, 150)?.target).toEqual(page(30));
		// Its edges are part of it; the gap between two links is not.
		expect(linkAt(links, 60, 100)?.target).toEqual(page(14));
		expect(linkAt(links, 100, 133)).toBeNull();
		expect(linkAt(links, 20, 110)).toBeNull();
		expect(linkAt([], 100, 110)).toBeNull();
	});

	it('is the one on top where two overlap', () => {
		expect(linkAt(links, 350, 110)?.target).toEqual(page(58));
	});
});

describe('following a link or a bookmark', () => {
	const goToPoint = vi.fn();
	const tab = { id: 7, viewer: { goToPoint } } as unknown as DocTab;

	it('goes to a place in this document, and says nothing', () => {
		follow(tab, { kind: 'page', page: 14, x: 0, y: 0.12, named: null }, ON_PAGE);
		expect(goToPoint).toHaveBeenCalledWith(14, 0, 0.12, { smooth: true });
		expect(store.notify).not.toHaveBeenCalled();
		expect(ipc.openLink).not.toHaveBeenCalled();
		expect(targetNotice(page(3), 'link')).toBeNull();
		expect(targetNotice({ kind: 'none' }, 'link')).toBeNull();
	});

	it('glides to the place of a bookmark or a link, unless Settings turn that off', () => {
		follow(tab, page(30), BOOKMARK);
		expect(goToPoint).toHaveBeenLastCalledWith(30, null, null, { smooth: true });
		store.settings = { smoothBookmarkScroll: false };
		follow(tab, page(30), BOOKMARK);
		expect(goToPoint).toHaveBeenLastCalledWith(30, null, null, { smooth: false });
		follow(tab, page(31), ON_PAGE);
		expect(goToPoint).toHaveBeenLastCalledWith(31, null, null, { smooth: false });
		store.settings = { smoothBookmarkScroll: true };
		follow(tab, page(30), BOOKMARK);
		expect(goToPoint).toHaveBeenLastCalledWith(30, null, null, { smooth: true });
		follow(tab, page(31), ON_PAGE);
		expect(goToPoint).toHaveBeenLastCalledWith(31, null, null, { smooth: true });
		store.settings = null;
	});

	it('opens a web address in the browser, naming where the document has it, and says which', async () => {
		follow(tab, { kind: 'uri', uri: 'https://example.org/a' }, ON_PAGE);
		expect(ipc.openLink).toHaveBeenCalledWith(7, ON_PAGE, 'https://example.org/a');
		await vi.waitFor(() => expect(store.notify).toHaveBeenCalled());
		expect(store.notify.mock.calls[0]?.[0]).toMatchObject({ message: 'Opened in your browser', suggestion: 'https://example.org/a' });
		expect(goToPoint).not.toHaveBeenCalled();
		// A bookmark's web address opens the same way.
		follow(tab, { kind: 'uri', uri: 'http://example.org' }, BOOKMARK);
		expect(ipc.openLink).toHaveBeenLastCalledWith(7, BOOKMARK, 'http://example.org');
	});

	it('offers to copy an address the browser could not be given', async () => {
		ipc.openLink.mockRejectedValueOnce({ message: 'The browser couldn’t be opened.', suggestion: null, code: 'general' });
		follow(tab, { kind: 'uri', uri: 'https://example.org/a' }, ON_PAGE);
		await vi.waitFor(() => expect(store.showError).toHaveBeenCalled());
		expect(store.showError.mock.calls[0]?.[1]?.label).toBe('Copy link');
		expect(store.notify).not.toHaveBeenCalled();
	});

	it('only opens web addresses: the rest is described, with a way to copy it', () => {
		for (const uri of ['https://example.org/a', 'http://example.org', 'HTTPS://EXAMPLE.ORG/x?y=z']) expect(isWebAddress(uri)).toBe(true);
		for (const uri of ['mailto:a@example.org', 'file:///C:/Windows/notepad.exe', 'javascript:alert(1)', 'example.org', 'https://', 'https://a b', '']) {
			expect(isWebAddress(uri), uri).toBe(false);
		}
		follow(tab, { kind: 'uri', uri: 'mailto:a@example.org' }, ON_PAGE);
		expect(ipc.openLink).not.toHaveBeenCalled();
		const toast = store.notify.mock.calls[0]?.[0];
		expect(toast.message).toBe('This link leads to: mailto:a@example.org');
		expect(toast.action.label).toBe('Copy link');
		follow(tab, { kind: 'uri', uri: 'mailto:a@example.org' }, BOOKMARK);
		expect(store.notify.mock.calls[1]?.[0].message).toBe('This bookmark leads to: mailto:a@example.org');
	});

	it('says what it can’t follow', () => {
		expect(targetNotice({ kind: 'broken', named: null }, 'link')).toEqual({
			kind: 'error',
			message: 'This link’s destination doesn’t exist in this document.',
			suggestion: undefined
		});
		expect(targetNotice({ kind: 'broken', named: 'x' }, 'bookmark')?.suggestion).toContain('Set destination');
		expect(targetNotice({ kind: 'file', file: 'other.pdf' }, 'link')?.message).toBe('This link opens another file: other.pdf.');
		expect(targetNotice({ kind: 'action', action: 'JavaScript' }, 'bookmark')?.message).toBe(
			'This bookmark runs a JavaScript action, which Lectrix doesn’t do.'
		);
		follow(tab, { kind: 'file', file: 'other.pdf' }, BOOKMARK);
		expect(store.notify.mock.calls[0]?.[0].message).toBe('This bookmark opens another file: other.pdf.');
	});
});

describe('where a link leads, before it is clicked', () => {
	it('is a page, named as the page box names it', () => {
		expect(linkPreview(page(175), null)).toBe('Page 176');
		// With page labels: the label of that page.
		expect(linkPreview(page(1), ['Cover', 'i', 'ii'])).toBe('Page i');
		expect(linkPreview(page(5), ['Cover', 'i'])).toBe('Page 6');
	});

	it('is an address, shortened when very long', () => {
		expect(linkPreview({ kind: 'uri', uri: 'https://example.org/a' }, null)).toBe('https://example.org/a');
		expect(linkPreview({ kind: 'uri', uri: 'mailto:a@example.org' }, null)).toBe('mailto:a@example.org');
		const long = linkPreview({ kind: 'uri', uri: `https://example.org/${'a'.repeat(400)}` }, null);
		expect(long).toHaveLength(161);
		expect(long.endsWith('…')).toBe(true);
	});

	it('or what else the link does', () => {
		expect(linkPreview({ kind: 'file', file: 'appendix.pdf' }, null)).toBe('Another file: appendix.pdf');
		expect(linkPreview({ kind: 'file', file: '' }, null)).toBe('Another file');
		expect(linkPreview({ kind: 'action', action: 'JavaScript' }, null)).toBe('JavaScript action, which Lectrix doesn’t run');
		expect(linkPreview({ kind: 'broken', named: null }, null)).toBe('Leads nowhere in this document');
		expect(linkPreview({ kind: 'none' }, null)).toBe('');
	});
});
