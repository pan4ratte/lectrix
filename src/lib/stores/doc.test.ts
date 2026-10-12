import { describe, expect, it, vi } from 'vitest';

import type { DocumentInfo, PdfaLevel, SaveResult } from '#lib/ipc/index.ts';

vi.mock('#lib/ipc/index.ts', () => ({ getPageText: vi.fn(), searchText: vi.fn(), logError: vi.fn(async () => {}) }));

const { DocTab } = await import('./doc.svelte.ts');

const PDFA_1B: PdfaLevel = { part: 1, conformance: 'B' };

/** What Rust reports for a file: one that claims PDF/A, opened for reading only or not. */
function info(pdfa: PdfaLevel | null, readOnly = pdfa !== null): DocumentInfo {
	return {
		id: 7,
		name: 'archive.pdf',
		path: 'C:\\docs\\archive.pdf',
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
	const view = { page: 0, offset: 0, zoom: 1, zoomMode: 'custom' as const, rotation: 0 };

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
