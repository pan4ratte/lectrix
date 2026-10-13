// App commands, shared by the menus and the keyboard shortcuts (section 8).

import { repair } from '#lib/features/annotations/actions.ts';
import { tools } from '#lib/features/annotations/state.svelte.ts';
import type { Tool } from '#lib/features/annotations/tools.ts';
import { addBookmark } from '#lib/features/bookmarks/actions.ts';
import { openWhatsNew } from '#lib/features/changelog/announce.ts';
import { toggleFullScreen } from '#lib/features/fullscreen/actions.ts';
import { showLabels } from '#lib/features/labels/actions.ts';
import { insertFromFile, toggleOrganize } from '#lib/features/organize/actions.ts';
import { update } from '#lib/features/update/update.svelte.ts';
import { copySelection } from '#lib/features/viewer/actions.ts';
import { normalizeRotation, type ViewMode } from '#lib/features/viewer/layout.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

function withTab(run: (tab: DocTab) => void | Promise<unknown>) {
	return () => {
		const tab = app.active;
		if (tab) void run(tab);
	};
}

/** A command for a document being read: it does nothing while the document's pages are
 * organized (section 6.7), where the viewer and the side panes are not on screen. */
function reading(run: (tab: DocTab) => void | Promise<unknown>) {
	return withTab((t) => {
		if (!t.organize) return run(t);
	});
}

/** Lays the active document's pages out in `mode` (section 6.1); not stored in the file. */
export function setViewMode(mode: ViewMode) {
	const tab = app.active;
	if (tab) layOut(tab, mode, tab.cover);
}

function layOut(tab: DocTab, mode: ViewMode, cover: boolean) {
	if (tab.viewer) {
		tab.viewer.setMode(mode, cover);
	} else {
		tab.mode = mode;
		tab.cover = cover;
	}
}

/** Zooms the pages in or out. In a grid of pages (the Combine view's, or that of a document
 * whose pages are being organized), the pictures get larger and smaller. */
function zoom(direction: 1 | -1) {
	if (app.combineActive) app.combine?.zoom.zoomStep(direction);
	else if (app.active?.organize) app.active.organize.zoom.zoomStep(direction);
	else app.active?.viewer?.zoomStep(direction);
}

/** Picks an annotation tool (section 8 shortcuts). Drawing tools need permission to annotate,
 * and a document that can be changed: one open for reading only says how to change that. */
function useTool(tool: Tool) {
	return withTab((t) => {
		// The grid of pages being organized has no annotation tools.
		if (t.organize) return;
		if (tool !== 'select' && (app.refuseReadOnly(t) || !t.flags.canAnnotate)) return;
		tools.tool = tool;
	});
}

export const commands = {
	open: () => void app.open(),
	save: withTab((t) => app.save(t)),
	saveAs: withTab((t) => app.saveAs(t)),
	saveAsOptimized: withTab((t) => app.saveAs(t, true)),
	reload: withTab((t) => app.reload(t)),
	closeTab: () => {
		if (app.combineActive) void app.closeCombine();
		else if (app.active) void app.closeTab(app.active.id);
	},
	combineFiles: () => app.openCombine(),
	/** The active tab into a window of its own (section 8). */
	moveTabToNewWindow: withTab((t) => {
		if (!app.isOnlyTab(t.id)) void app.moveToWindow(t.id, { kind: 'new' });
	}),
	exit: () => void app.exit(),
	settings: () => {
		app.settingsOpen = true;
	},
	about: () => {
		app.aboutOpen = true;
	},
	whatsNew: openWhatsNew,
	checkForUpdates: () => void update.checkNow(),

	undo: () => {
		if (app.combineActive) app.combine?.undo();
		else if (app.active) void app.undo(app.active);
	},
	redo: () => {
		if (app.combineActive) app.combine?.redo();
		else if (app.active) void app.redo(app.active);
	},
	copy: withTab((t) => copySelection(t)),
	find: reading((t) => {
		if (t.search.open) {
			const input = document.querySelector<HTMLInputElement>('[data-search-input]');
			input?.focus();
			input?.select();
		} else {
			t.search.open = true;
		}
	}),
	findNext: reading((t) => t.search.step(1)),
	findPrevious: reading((t) => t.search.step(-1)),

	zoomIn: () => zoom(1),
	zoomOut: () => zoom(-1),
	fitWidth: reading((t) => t.viewer?.fit('fitWidth')),
	fitPage: reading((t) => t.viewer?.fit('fitPage')),
	rotateViewClockwise: reading((t) => {
		t.rotation = normalizeRotation(t.rotation + 90);
	}),
	rotateViewCounterClockwise: reading((t) => {
		t.rotation = normalizeRotation(t.rotation - 90);
	}),
	/** In the two-page modes, gives the first page a row of its own, or not. */
	toggleCoverPage: reading((t) => layOut(t, t.mode, !t.cover)),
	/** The pages alone, on the whole screen (section 6.1), and back. */
	fullScreen: () => {
		// Full screen shows the pages alone; the grid of pages being organized is not that.
		if (!app.active?.organize) void toggleFullScreen();
	},
	toggleLeftPane: reading(() => app.togglePane('left')),
	toggleRightPane: reading(() => app.togglePane('right')),
	goToPage: () => {
		const box = document.querySelector<HTMLInputElement>('[data-page-box]');
		box?.focus();
		box?.select();
	},
	back: reading((t) => {
		const target = t.viewer && t.history.goBack(t.viewer.position());
		if (target) t.viewer?.goTo(target, { recordHistory: false });
	}),
	forward: reading((t) => {
		const target = t.viewer && t.history.goForward(t.viewer.position());
		if (target) t.viewer?.goTo(target, { recordHistory: false });
	}),
	nextTab: () => app.cycleTab(1),
	previousTab: () => app.cycleTab(-1),

	/** The pages as a grid to organize, in place of the viewer (section 6.7), and back. */
	organizePages: withTab((t) => toggleOrganize(t)),
	insertPages: withTab((t) => insertFromFile(t)),
	addBookmark: reading((t) => addBookmark(t)),
	showPages: reading(() => app.showPanel('pages')),
	showBookmarks: reading(() => app.showPanel('bookmarks')),
	showAnnotations: reading(() => app.showPanel('annotations')),
	repairAnnotations: withTab((t) => repair(t)),
	toolSelect: useTool('select'),
	toolHighlight: useTool('highlight'),
	toolUnderline: useTool('underline'),
	toolStrikeOut: useTool('strikeOut'),
	toolSquiggly: useTool('squiggly'),
	toolNote: useTool('note'),
	toolPen: useTool('ink'),
	toolText: useTool('freeText'),
	/** The Page labels dialog (section 6.3). It belongs to the grid of pages being organized
	 * (section 6.7), for the pages selected there: it does nothing while a document is read. */
	showLabels: withTab((t) => showLabels(t))
};

export type CommandName = keyof typeof commands;
