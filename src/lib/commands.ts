// App commands, shared by the menus and the keyboard shortcuts (section 8).

import { getCurrentWindow } from '@tauri-apps/api/window';

import { repair } from '#lib/features/annotations/actions.ts';
import { tools } from '#lib/features/annotations/state.svelte.ts';
import type { Tool } from '#lib/features/annotations/tools.ts';
import { addBookmark } from '#lib/features/bookmarks/actions.ts';
import { showLabels, startRangeAt } from '#lib/features/labels/actions.ts';
import { copySelection } from '#lib/features/viewer/actions.ts';
import { normalizeRotation } from '#lib/features/viewer/layout.ts';
import { stepZoom } from '#lib/features/viewer/zoom.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

function withTab(run: (tab: DocTab) => void | Promise<unknown>) {
	return () => {
		const tab = app.active;
		if (tab) void run(tab);
	};
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

	zoomIn: withTab((t) => t.viewer?.setZoom(stepZoom(t.zoom, 1), 'custom')),
	zoomOut: withTab((t) => t.viewer?.setZoom(stepZoom(t.zoom, -1), 'custom')),
	fitWidth: withTab((t) => t.viewer?.fit('fitWidth')),
	fitPage: withTab((t) => t.viewer?.fit('fitPage')),
	rotateViewClockwise: withTab((t) => {
		t.rotation = normalizeRotation(t.rotation + 90);
	}),
	rotateViewCounterClockwise: withTab((t) => {
		t.rotation = normalizeRotation(t.rotation - 90);
	}),
	toggleSidebar: () => {
		app.sidebarOpen = !app.sidebarOpen;
	},
	toggleAnnotations: () => {
		app.annotationsOpen = !app.annotationsOpen;
	},
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
	showBookmarks: () => {
		app.sidebarOpen = true;
		app.sidebarPanel = 'bookmarks';
	},
	showLabels: () => showLabels(),
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
