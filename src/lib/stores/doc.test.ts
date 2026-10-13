import { describe, expect, it, vi } from 'vitest';

import type { DocumentChange, DocumentInfo, PdfaLevel, SaveResult } from '#lib/ipc/index.ts';

vi.mock('#lib/ipc/index.ts', () => ({ getPageText: vi.fn(), searchText: vi.fn(), logError: vi.fn(async () => {}) }));

const { DocTab } = await import('./doc.svelte.ts');

const PDFA_1B: PdfaLevel = { part: 1, conformance: 'B' };

/** What Rust reports for a file: one that claims PDF/A, opened for reading only or not. */
function info(pdfa: PdfaLevel | null, readOnly = pdfa !== null): DocumentInfo {
	return {
		id: 7,
		name: 'archive.pdf',
		path: 'C:\\docs\\archive.pdf',
		untitled: false,
		pages: [{ width: 612, height: 792, rotation: 0 }],
		labels: null,
		labelRules: [],
		flags: { encrypted: false, signed: false, repaired: false, canAssemble: true, canAnnotate: true, canCopy: true, pdfa },
		readOnly,
		pageLayout: null,
		state: { revision: 0, dirty: false, undoName: null, redoName: null },
		outline: { items: [], damaged: false },
		annotations: [],
		view: null,
		openMs: 1
	};
}

function saved(pdfaRemoved: PdfaLevel | null): SaveResult {
	return {
		state: { revision: 1, dirty: false, undoName: null, redoName: null },
		name: 'archive.pdf',
		path: 'C:\\docs\\archive.pdf',
		fellBackToFull: false,
		outline: null,
		annotations: [],
		pdfaRemoved
	};
}

describe('a document Rust opened for reading only', () => {
	it('offers nothing that changes it, and the rest stays allowed', () => {
		const tab = new DocTab(info(PDFA_1B));
		expect(tab.readOnly).toBe(true);
		expect(tab.flags).toMatchObject({ canAnnotate: false, canAssemble: false, canCopy: true });
		expect(tab.canEditBookmarks).toBe(false);
		expect(tab.canEditLabels).toBe(false);
		// What the file itself allows is still known.
		expect(tab.fileFlags).toMatchObject({ canAnnotate: true, canAssemble: true });
	});

	it('can be changed once editing is enabled', () => {
		const tab = new DocTab(info(PDFA_1B));
		tab.readOnly = false;
		expect(tab.flags).toMatchObject({ canAnnotate: true, canAssemble: true, pdfa: PDFA_1B });
		expect(tab.canEditBookmarks).toBe(true);
	});

	it('follows Rust when the file is reloaded', () => {
		const tab = new DocTab(info(PDFA_1B));
		tab.update(info(PDFA_1B, false));
		expect(tab.readOnly).toBe(false);
		tab.update(info(PDFA_1B, true));
		expect(tab.readOnly).toBe(true);
		// Reloaded without its claim, it is a file like any other.
		tab.update(info(null));
		expect(tab.readOnly).toBe(false);
		expect(tab.flags.canAnnotate).toBe(true);
	});
});

describe('a file that claims PDF/A, opened ready to edit', () => {
	it('is a document like any other', () => {
		const tab = new DocTab(info(PDFA_1B, false));
		expect(tab.readOnly).toBe(false);
		expect(tab.flags).toMatchObject({ canAnnotate: true, canAssemble: true, pdfa: PDFA_1B });
	});

	it('keeps its claim through a save that keeps it, and loses it with one that removes it', () => {
		const tab = new DocTab(info(PDFA_1B, false));
		tab.applySaved(saved(null));
		expect(tab.flags.pdfa).toEqual(PDFA_1B);
		tab.applySaved(saved(PDFA_1B));
		expect(tab.flags.pdfa).toBeNull();
		expect(tab.fileFlags.pdfa).toBeNull();
	});
});

describe('any other file', () => {
	it('opens ready to edit', () => {
		const tab = new DocTab(info(null));
		expect(tab.readOnly).toBe(false);
		expect(tab.flags).toMatchObject({ canAnnotate: true, canAssemble: true });
	});
});

describe('visual adjustments', () => {
	const view = { page: 0, offset: 0, zoom: 1, zoomMode: 'custom' as const, rotation: 0, pageTurns: {} };

	it('start with none, and with the ones remembered for the file', () => {
		const fresh = new DocTab(info(null));
		expect(fresh.pageFilter).toBeNull();
		expect(fresh.viewState().adjustments).toEqual({ brightness: 100, contrast: 100, invert: false, grayscale: false });

		const adjustments = { brightness: 80, contrast: 100, invert: true, grayscale: false };
		const tab = new DocTab({ ...info(null), view: { ...view, adjustments } });
		expect(tab.adjustments).toEqual(adjustments);
		expect(tab.pageFilter).toBe('invert(1) hue-rotate(180deg) brightness(0.8)');
	});

	it('go back to Rust with the view, for the next time the file opens', () => {
		const tab = new DocTab(info(null));
		tab.adjustments.grayscale = true;
		tab.adjustments.contrast = 120;
		expect(tab.pageFilter).toBe('grayscale(1) contrast(1.2)');
		const remembered = tab.viewState().adjustments;
		expect(remembered).toEqual({ brightness: 100, contrast: 120, invert: false, grayscale: true });
		// A copy: what the tab changes later is not what was sent.
		tab.adjustments.grayscale = false;
		expect(remembered.grayscale).toBe(true);
	});

	it('are none for a view remembered before there were any', () => {
		const old = { ...view } as DocumentInfo['view'];
		expect(new DocTab({ ...info(null), view: old }).adjustments).toEqual({ brightness: 100, contrast: 100, invert: false, grayscale: false });
	});

	it('have nothing to do with a file open for reading only', () => {
		const tab = new DocTab(info(PDFA_1B));
		tab.adjustments.invert = true;
		expect(tab.pageFilter).toBe('invert(1) hue-rotate(180deg)');
		expect(tab.readOnly).toBe(true);
	});
});

describe('pages turned as they are shown', () => {
	const size = { width: 612, height: 792, rotation: 0 };
	const adjustments = { brightness: 100, contrast: 100, invert: false, grayscale: false };
	const view = { page: 0, offset: 0, zoom: 1, zoomMode: 'custom' as const, rotation: 0, adjustments, pageTurns: {} };
	const three = () => new DocTab({ ...info(null), pages: [size, size, size] });

	it('turn by quarters on top of the view rotation, and back to none', () => {
		const tab = three();
		tab.turnPages([1], 90);
		tab.turnPages([1, 2], -90);
		expect([0, 1, 2].map((p) => tab.shownRotation(p))).toEqual([0, 0, 270]);
		expect([...tab.pageTurns]).toEqual([[2, 270]]);
		tab.rotation = 180;
		expect([0, 1, 2].map((p) => tab.shownRotation(p))).toEqual([180, 180, 90]);
		// A page that is not there is not turned.
		tab.turnPages([3, -1], 90);
		expect([...tab.pageTurns]).toEqual([[2, 270]]);
	});

	it('change nothing in the document, in one open for reading only either', () => {
		const tab = new DocTab(info(PDFA_1B));
		tab.turnPages([0], 90);
		expect(tab.shownRotation(0)).toBe(90);
		expect(tab.state).toEqual({ revision: 0, dirty: false, undoName: null, redoName: null });
		expect(tab.pages[0]).toEqual(size);
		expect(tab.readOnly).toBe(true);
	});

	it('go to Rust with the view, and come back with it', () => {
		const tab = three();
		tab.turnPages([0], 180);
		tab.turnPages([2], 90);
		expect(tab.viewState().pageTurns).toEqual({ 0: 180, 2: 90 });
		// What Rust stored for the file: a turn that is none, one that is not a quarter turn
		// and one for a page the file no longer has are left out.
		const again = new DocTab({
			...info(null),
			pages: [size, size, size],
			view: { ...view, pageTurns: { 0: 180, 1: 0, 2: 450, 7: 90 } }
		});
		expect([...again.pageTurns]).toEqual([
			[0, 180],
			[2, 90]
		]);
		// A view remembered before pages could be turned has none.
		const { pageTurns: _, ...old } = view;
		expect(new DocTab({ ...info(null), view: old as DocumentInfo['view'] }).pageTurns.size).toBe(0);
	});

	it('follow their pages when pages are moved or deleted, and end with a shorter file', () => {
		const tab = three();
		tab.turnPages([0], 90);
		tab.turnPages([2], 180);
		// The third page to the front, then that page deleted.
		const moved = (pageMap: (number | null)[]): DocumentChange => ({
			state: { revision: tab.state.revision + 1, dirty: true, undoName: 'Move page', redoName: null },
			changedPages: [],
			labelsChanged: false,
			labels: null,
			labelRules: [],
			outline: null,
			created: null,
			pageCount: pageMap.length,
			mergeReport: null,
			annotations: [],
			pageMap,
			deleted: null
		});
		tab.applyChange(moved([2, 0, 1]));
		expect(Object.fromEntries(tab.pageTurns)).toEqual({ 0: 180, 1: 90 });
		tab.applyChange(moved([1, 2]));
		expect(Object.fromEntries(tab.pageTurns)).toEqual({ 0: 90 });
		// Read anew with one page: the turn of a page past its end goes.
		tab.turnPages([1], 270);
		tab.update({ ...info(null), pages: [size] });
		expect(Object.fromEntries(tab.pageTurns)).toEqual({ 0: 90 });
	});
});

describe('pages moved, deleted or inserted', () => {
	const size = { width: 612, height: 792, rotation: 0 };
	const wide = { width: 792, height: 612, rotation: 0 };

	/** A five-page document, shown at the middle of its third page. */
	function five() {
		const tab = new DocTab({ ...info(null), pages: [size, size, wide, size, size] });
		tab.currentPage = 2;
		tab.pendingPosition = { page: 2, offset: 0.5 };
		return tab;
	}

	/** What Rust reports: the pages now by where they were, and the sizes that differ. */
	function change(pageMap: (number | null)[], changedPages: DocumentChange['changedPages']): DocumentChange {
		return {
			state: { revision: 1, dirty: true, undoName: 'Move page', redoName: null },
			changedPages,
			labelsChanged: false,
			labels: null,
			labelRules: [],
			outline: null,
			created: null,
			pageCount: pageMap.length,
			mergeReport: null,
			annotations: [],
			pageMap,
			deleted: null
		};
	}

	it('keep their keys, and the view stays on its page', () => {
		const tab = five();
		const keys = [...tab.pageKeys];
		expect(new Set(keys).size).toBe(5);
		// The third page to the front.
		tab.applyChange(
			change(
				[2, 0, 1, 3, 4],
				[
					{ index: 0, size: wide },
					{ index: 2, size }
				]
			)
		);
		expect(tab.pageKeys).toEqual([keys[2], keys[0], keys[1], keys[3], keys[4]]);
		expect(tab.pages[0]).toEqual(wide);
		expect(tab.currentPage).toBe(0);
		expect(tab.pendingPosition).toEqual({ page: 0, offset: 0.5 });
	});

	it('leave the view on the page that took the place of a deleted one', () => {
		const tab = five();
		const keys = [...tab.pageKeys];
		tab.history.push({ page: 2, offset: 0 });
		tab.history.push({ page: 4, offset: 0.25 });
		// The second and third pages are deleted.
		tab.applyChange(change([0, 3, 4], [{ index: 1, size }, { index: 2, size }]));
		expect(tab.pageCount).toBe(3);
		expect(tab.pageKeys).toEqual([keys[0], keys[3], keys[4]]);
		expect(tab.currentPage).toBe(2);
		expect(tab.pendingPosition).toEqual({ page: 2, offset: 0 });
		// A jump to a page that is gone is forgotten; the other follows its page.
		expect(tab.history.goBack({ page: 0, offset: 0 })).toEqual({ page: 2, offset: 0.25 });
		expect(tab.history.canGoBack).toBe(false);
	});

	it('give new pages keys of their own, and a reloaded file all new ones', () => {
		const tab = five();
		const keys = [...tab.pageKeys];
		tab.applyChange(change([0, null, 1, 2, 3, 4], [{ index: 1, size }, { index: 3, size: wide }, { index: 2, size }]));
		expect(tab.pageKeys.length).toBe(6);
		expect(tab.pageKeys[0]).toBe(keys[0]);
		expect(tab.pageKeys[2]).toBe(keys[1]);
		expect(keys).not.toContain(tab.pageKeys[1]);
		expect(tab.currentPage).toBe(3);

		const before = [...tab.pageKeys];
		tab.update({ ...info(null), pages: [size, size] });
		expect(tab.pageKeys.length).toBe(2);
		expect(before).not.toContain(tab.pageKeys[0]);
	});
});
