// App commands, shared by the menus and the keyboard shortcuts (section 8).

import { getCurrentWindow } from '@tauri-apps/api/window';

import { repair } from '#lib/features/annotations/actions.ts';
import { tools } from '#lib/features/annotations/state.svelte.ts';
import type { Tool } from '#lib/features/annotations/tools.ts';
import { addBookmark } from '#lib/features/bookmarks/actions.ts';
import { showLabels, startRangeAt } from '#lib/features/labels/actions.ts';
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

/** Picks an annotation tool (section 8 shortcuts). Drawing tools need permission to annotate. */
function useTool(tool: Tool) {
	return withTab((t) => {
		if (tool !== 'select' && !t.flags.canAnnotate) return;
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
	exit: () => void getCurrentWindow().close(),
	settings: () => {
		app.settingsOpen = true;
	},
	about: () => {
		app.aboutOpen = true;
	},
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
	find: withTab((t) => {
		if (t.search.open) {
			const input = document.querySelector<HTMLInputElement>('[data-search-input]');
			input?.focus();
			input?.select();
		} else {
			t.search.open = true;
		}
	}),
	findNext: withTab((t) => t.search.step(1)),
	findPrevious: withTab((t) => t.search.step(-1)),

	zoomIn: withTab((t) => t.viewer?.zoomStep(1)),
	zoomOut: withTab((t) => t.viewer?.zoomStep(-1)),
	fitWidth: withTab((t) => t.viewer?.fit('fitWidth')),
	fitPage: withTab((t) => t.viewer?.fit('fitPage')),
	rotateViewClockwise: withTab((t) => {
		t.rotation = normalizeRotation(t.rotation + 90);
	}),
	rotateViewCounterClockwise: withTab((t) => {
		t.rotation = normalizeRotation(t.rotation - 90);
	}),
	/** In the two-page modes, gives the first page a row of its own, or not. */
	toggleCoverPage: withTab((t) => layOut(t, t.mode, !t.cover)),
	toggleLeftPane: () => app.togglePane('left'),
	toggleRightPane: () => app.togglePane('right'),
	goToPage: () => {
		const box = document.querySelector<HTMLInputElement>('[data-page-box]');
		box?.focus();
		box?.select();
	},
	back: withTab((t) => {
		const target = t.viewer && t.history.goBack(t.viewer.position());
		if (target) t.viewer?.goTo(target, { recordHistory: false });
	}),
	forward: withTab((t) => {
		const target = t.viewer && t.history.goForward(t.viewer.position());
		if (target) t.viewer?.goTo(target, { recordHistory: false });
	}),
	nextTab: () => app.cycleTab(1),
	previousTab: () => app.cycleTab(-1),

	rotatePages: withTab(() => {
		app.rotateDialogOpen = true;
	}),
	insertPages: withTab((t) => app.insertFromFile(t)),
	addBookmark: withTab((t) => addBookmark(t)),
	showPages: () => app.showPanel('pages'),
	showBookmarks: () => app.showPanel('bookmarks'),
	showLabels: () => showLabels(),
	showAnnotations: () => app.showPanel('annotations'),
	repairAnnotations: withTab((t) => repair(t)),
	toolSelect: useTool('select'),
	toolHighlight: useTool('highlight'),
	toolUnderline: useTool('underline'),
	toolStrikeOut: useTool('strikeOut'),
	toolSquiggly: useTool('squiggly'),
	toolNote: useTool('note'),
	toolPen: useTool('ink'),
	toolText: useTool('freeText'),
	startLabelRange: withTab((t) => startRangeAt(t, t.currentPage))
};

export type CommandName = keyof typeof commands;
