// App commands, shared by the menus and the keyboard shortcuts (section 8).

import { copyInContext, refuseIfLocked, repair } from '#lib/features/annotations/actions.ts';
import { tools } from '#lib/features/annotations/state.svelte.ts';
import type { Tool } from '#lib/features/annotations/tools.ts';
import { addBookmark } from '#lib/features/bookmarks/actions.ts';
import { openWhatsNew } from '#lib/features/changelog/announce.ts';
import { toggleFullScreen } from '#lib/features/fullscreen/actions.ts';
import { showLabels } from '#lib/features/labels/actions.ts';
import { toggleTheme } from '#lib/features/settings/apply.ts';
import { insertFromFile, toggleOrganize } from '#lib/features/organize/actions.ts';
import { openProperties } from '#lib/features/properties/actions.ts';
import { openPrint } from '#lib/features/print/actions.ts';
import { compressDocument, compressFile } from '#lib/features/compress/actions.ts';
import { protectDocument, protectFile } from '#lib/features/protect/actions.ts';
import { unlockDocument, unlockFile } from '#lib/features/unprotect/actions.ts';
import { update } from '#lib/features/update/update.svelte.ts';
import { normalizeRotation, type ViewMode } from '#lib/features/viewer/layout.ts';
import { find, findStep } from '#lib/features/viewer/search.ts';
import { MAX_PANES } from '#lib/features/split/split.ts';
import { toggleLock, toggleZoomLock } from '#lib/features/split/sync.ts';
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
/** Shows the tab after the one shown (the one before, from the last) beside it (section
 * 6.1, split view). */
function joinNextTab() {
	const group = app.activeGroup;
	if (!group || app.combineActive || app.startShown) return;
	const i = app.groups.indexOf(group);
	const next = app.groups[i + 1] ?? app.groups[i - 1];
	if (next && !app.joinTabs(group, next)) tooManyPanes();
}

/** Says why a document is not put beside the others. */
export function tooManyPanes() {
	app.notify({
		kind: 'info',
		message: `A tab shows at most ${MAX_PANES} documents side by side.`,
		suggestion: 'Close one of them first, or open the document in a tab of its own.'
	});
}

/** Puts the focus on the next or the previous pane of the tab shown, and the keyboard's. */
function stepPane(direction: 1 | -1) {
	const group = app.activeGroup;
	if (!group?.joined || app.combineActive || app.startShown) return;
	const n = group.views.length;
	const view = group.views[(group.focus + direction + n) % n];
	if (!view) return;
	app.focusView(view);
	view.viewer?.focus();
}

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
		if (tool !== 'select' && refuseIfLocked(t)) return;
		tools.tool = tool;
	});
}

export const commands = {
	open: () => void app.open(),
	save: withTab((t) => app.save(t)),
	saveAs: withTab((t) => app.saveAs(t)),
	saveAsOptimized: withTab((t) => app.saveAs(t, true)),
	reload: withTab((t) => app.reload(t)),
	/** Closes the tab shown; in a joined tab, the pane in focus (section 6.1, split view). */
	closeTab: () => {
		if (app.combineActive) void app.closeCombine();
		else if (app.active && app.activeGroup?.joined) void app.closePane(app.active);
		else if (app.active) void app.closeTab(app.active.id);
	},
	combineFiles: () => app.openCombine(),
	/** A PDF without its password and restrictions (section 6.8): the protected document
	 * shown, or else a file the Open dialog asks for. */
	unlockFile: () => {
		const tab = app.combineActive || app.startShown ? null : app.active;
		if (tab?.flags.encrypted) void unlockDocument(tab);
		else void unlockFile();
	},
	/** A password, restrictions or both (section 6.9): for the document shown, saved with
	 * it next; or else for a file the Open dialog asks for. */
	protectFile: () => {
		const tab = app.combineActive || app.startShown ? null : app.active;
		if (tab) void protectDocument(tab);
		else void protectFile();
	},
	/** A PDF written smaller (section 6.11): the document shown's file, or else a file the
	 * Open dialog asks for. */
	compressFile: () => {
		const tab = app.combineActive || app.startShown ? null : app.active;
		if (tab) void compressDocument(tab);
		else void compressFile();
	},
	/** The Print dialog for the document shown (section 6.12). */
	print: withTab(openPrint),
	/** The active tab into a window of its own (section 8). */
	moveTabToNewWindow: withTab(() => {
		const group = app.activeGroup;
		if (group && !app.isOnlyGroup(group)) void app.moveGroupToWindow(group, { kind: 'new' });
	}),
	exit: () => void app.exit(),
	// Their shortcut pressed again, while they are open, closes them.
	settings: () => {
		app.settingsOpen = !app.settingsOpen;
	},
	about: () => {
		app.aboutOpen = !app.aboutOpen;
	},
	whatsNew: () => {
		if (app.whatsNewOpen) app.whatsNewOpen = false;
		else openWhatsNew();
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
	copy: withTab((t) => copyInContext(t)),
	find: reading((t) => find(t)),
	findNext: reading((t) => findStep(t, 1)),
	findPrevious: reading((t) => findStep(t, -1)),

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
	/** The other theme, light or dark, from the one on screen. */
	toggleTheme: () => void toggleTheme(),
	toggleLeftPane: reading(() => app.togglePane('left')),
	toggleRightPane: reading(() => app.togglePane('right')),
	toggleSidePanes: reading(() => app.toggleSidePanes()),
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

	// Documents side by side (section 6.1, split view).
	showTwice: reading((t) => {
		if (!app.showTwice(t)) tooManyPanes();
	}),
	showNextTabBeside: reading(() => joinNextTab()),
	separateTabs: withTab(() => {
		const group = app.activeGroup;
		if (group?.joined) app.separate(group);
	}),
	nextPane: () => stepPane(1),
	previousPane: () => stepPane(-1),
	syncScrolling: reading(() => toggleLock()),
	syncZoom: reading(() => toggleZoomLock()),

	/** The pages as a grid to organize, in place of the viewer (section 6.7), and back. */
	organizePages: withTab((t) => toggleOrganize(t)),
	insertPages: withTab((t) => insertFromFile(t)),
	addBookmark: reading((t) => addBookmark(t)),
	showPages: reading(() => app.togglePanel('pages')),
	showBookmarks: reading(() => app.togglePanel('bookmarks')),
	showAnnotations: reading(() => app.togglePanel('annotations')),
	repairAnnotations: withTab((t) => repair(t)),
	documentProperties: withTab((t) => openProperties(t)),
	toolSelect: useTool('select'),
	toolHighlight: useTool('highlight'),
	toolUnderline: useTool('underline'),
	toolStrikeOut: useTool('strikeOut'),
	toolSquiggly: useTool('squiggly'),
	toolNote: useTool('note'),
	toolPen: useTool('ink'),
	toolText: useTool('freeText'),
	/** The number keys: a preset colour for new annotations, whatever the tool (section 6.5). */
	colour1: () => tools.pickPreset(1),
	colour2: () => tools.pickPreset(2),
	colour3: () => tools.pickPreset(3),
	colour4: () => tools.pickPreset(4),
	colour5: () => tools.pickPreset(5),
	colour6: () => tools.pickPreset(6),
	colour7: () => tools.pickPreset(7),
	colour8: () => tools.pickPreset(8),
	colour9: () => tools.pickPreset(9),
	/** The Page labels dialog (section 6.3). It belongs to the grid of pages being organized
	 * (section 6.7), for the pages selected there: it does nothing while a document is read. */
	showLabels: withTab((t) => showLabels(t))
};

export type CommandName = keyof typeof commands;
