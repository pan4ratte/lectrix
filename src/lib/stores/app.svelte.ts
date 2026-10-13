// App-wide state: tabs, dialogs, notifications, and the flows that span them (open, save,
// close, exit). Rust owns the documents; this owns what the window shows.

import { getCurrentWindow } from '@tauri-apps/api/window';

import {
	applyOperation,
	cancelUnlock,
	closeDocument,
	closeOtherWindows as closeOtherWindowsCommand,
	discardRecovered,
	enableEditing as enableEditingCommand,
	exitConfirmed,
	listOpenDocuments,
	listRecentFiles,
	listRecovered,
	listWindows,
	moveDocument,
	onCloseDeclined,
	openInsertSource,
	openRecent,
	openStartupDocuments,
	openWithDialog,
	recentChanged,
	redo as redoCommand,
	reloadDocument,
	rememberView,
	removeRecent,
	restoreRecovered,
	save as saveCommand,
	saveAs as saveAsCommand,
	setDropTarget,
	toAppError,
	undo as undoCommand,
	unlockDocument,
	type AppError,
	type BookmarkMode,
	type DocumentChange,
	type DocumentInfo,
	type InsertLabelMode,
	type MoveTarget,
	type OpenResult,
	type OperationInput,
	type PaneLayout,
	type PanelId,
	type PdfaLevel,
	type RecentFile,
	type UpdateInfo,
	type SaveResult,
	type Settings,
	type StartupInfo
} from '#lib/ipc/index.ts';
import {
	DEFAULT_ARRANGEMENT,
	LEFT_LIMITS,
	RIGHT_LIMITS,
	clampWidth,
	movePanel,
	normalizeArrangement,
	paneOf,
	type PaneSide,
	type PanelArrangement
} from '#lib/components/panes.ts';
import { fullscreen } from '#lib/features/fullscreen/fullscreen.svelte.ts';
import { CombineState } from '#lib/features/merge/combine.svelte.ts';
import { reportDetail, signedWarning } from '#lib/features/merge/pages.ts';
import { clampThumbWidth, DEFAULT_THUMB_WIDTH } from '#lib/features/viewer/thumbs.ts';
import { pdfaName } from '#lib/format.ts';
import { discardQuestion, recoveryQuestion } from '#lib/features/recovery/recovery.ts';
import type { CommandName } from '#lib/commands.ts';
import { shortcutOf } from '#lib/shortcuts.ts';

import { DocTab } from './doc.svelte.ts';

export interface DialogButton {
	id: string;
	label: string;
	primary?: boolean;
	/** The button does something that can't be undone. */
	danger?: boolean;
}

export interface DialogRequest {
	title: string;
	message: string;
	detail?: string;
	buttons: DialogButton[];
	/** The button chosen by Escape or closing the dialog. */
	cancel: string;
	resolve: (id: string) => void;
}

export interface PasswordRequest {
	token: number;
	name: string;
	retry: boolean;
	/** What the file is opened for: a tab, the Combine view, or inserting its pages. */
	purpose: 'open' | 'combine' | 'insert';
}

/** "Insert pages from file": the file chosen, waiting for the dialog's options. */
export interface InsertRequest {
	tabId: number;
	source: DocumentInfo;
	/** Suggested position: insert before this page (0-based; the page count appends). */
	at: number;
}

export interface Toast {
	id: number;
	kind: 'error' | 'info';
	message: string;
	suggestion?: string | null;
	action?: { label: string; run: () => void };
}

let toastSeq = 0;

/** What the annotation list shows of a document's annotations. */
export interface AnnotationRows {
	tab: DocTab;
	/** The search or the filters leave some annotations out. */
	readonly filtering: boolean;
	/** Whether the annotation has a row. */
	has(page: number, id: number): boolean;
}

class AppStore {
	tabs = $state<DocTab[]>([]);
	activeId = $state<number | null>(null);
	startup = $state<StartupInfo | null>(null);
	recent = $state<RecentFile[]>([]);
	/** The side panes (section 8): open, their widths, the panels each holds (the user
	 * moves them between the panes) and the one each shows. */
	leftOpen = $state(true);
	leftWidth = $state(LEFT_LIMITS.initial);
	rightOpen = $state(false);
	rightWidth = $state(RIGHT_LIMITS.initial);
	panels = $state<PanelArrangement>({ left: [...DEFAULT_ARRANGEMENT.left], right: [...DEFAULT_ARRANGEMENT.right] });
	/** The width chosen for page thumbnails (Smaller, Larger), CSS pixels. */
	thumbnailWidth = $state(DEFAULT_THUMB_WIDTH);
	/** Page thumbnails fill the Pages panel's width. */
	thumbnailsFit = $state(false);
	/** Page thumbnails in a grid of as many columns as fit, rather than one column. */
	thumbnailsGrid = $state(false);
	/** The start screen shows recent files as a grid of previews, rather than a table. */
	recentFilesGrid = $state(false);
	#leftActive = $state<PanelId | null>(null);
	#rightActive = $state<PanelId | null>(null);
	/** The remembered pane layout is in; until then panes neither animate nor are saved. */
	panesRestored = $state(false);
	/** The inspector (properties of the selected bookmark) is open. */
	inspectorOpen = $state(false);
	/** Whether the selected annotation ("tab:page:id") shows its comment panel (the
	 * inspector beside it) rather than its bar. Set when it is selected (open if it has a
	 * comment), opened from its bar, a double-click or the list, closed with Esc. While the
	 * annotation list shows the annotation, its row there stands in for the panel. */
	annotationPanel = $state<{ key: string; open: boolean } | null>(null);
	/** The Properties dialog of the selected annotation (author, dates) is open. */
	annotationPropertiesOpen = $state(false);
	/** A note was just placed: the annotation inspector focuses its text. */
	focusNoteText = $state(false);
	/** The annotation list focuses the selected annotation's comment: a double-click on it,
	 * or it was just made, while the list shows it. */
	focusListComment = $state(false);
	/** The rows of the annotation list, which its search and filters narrow down: set by the
	 * list while it is there, null without one. While the list is on screen, the annotations
	 * it leaves out are not drawn on the page either (`hiddenByList`). */
	annotationRows = $state.raw<AnnotationRows | null>(null);
	/** The Settings dialog is open. */
	settingsOpen = $state(false);
	/** The stored settings, once loaded. */
	settings = $state<Settings | null>(null);

	/** A command's first shortcut as Settings has it, for menus; '' without one. */
	shortcut(command: CommandName): string {
		return shortcutOf(command, this.settings?.shortcuts);
	}

	/** A control's name with its command's shortcut, for tooltips: "Zoom in (Ctrl+=)". */
	hint(label: string, command: CommandName): string {
		const keys = this.shortcut(command);
		return keys ? `${label} (${keys})` : label;
	}

	/** The edge of the page canvas the floating annotation toolbar stays at, or null: it is
	 * docked beside the canvas (unless full screen, where it floats whatever Settings say),
	 * or shows only while the pointer is near. */
	private floatingToolbarEdge = $derived(
		this.settings !== null &&
			(fullscreen.started || this.settings.toolbarStyle === 'floating') &&
			this.settings.toolbarVisibility === 'always'
			? this.settings.toolbarPosition
			: null
	);
	/** Where the search bar and inspectors start, CSS pixels from the top of the page
	 * canvas: below the floating annotation toolbar when it stays at the top. */
	overlayTop = $derived(this.floatingToolbarEdge === 'top' ? 64 : 12);
	/** And from the right of the page canvas: beside that toolbar when it stays at the right
	 * (stacked, 24.5 px from the edge and up to 51 px wide). */
	overlayRight = $derived(this.floatingToolbarEdge === 'right' ? 84 : 21);
	/** The About dialog is open. */
	aboutOpen = $state(false);
	/** What's new (the changelog, section 6.6) is open. */
	whatsNewOpen = $state(false);
	/** What's new shows the changes an update brings, before updating, rather than the
	 * changelog of the version running. */
	whatsNewUpdate = $state<UpdateInfo | null>(null);
	/** The version Lectrix was updated to, when it started on the start screen: shown there,
	 * beside the app's name, until dismissed or followed (section 6.6). */
	updatedTo = $state<string | null>(null);
	dialog = $state<DialogRequest | null>(null);
	passwordPrompts = $state<PasswordRequest[]>([]);
	toasts = $state<Toast[]>([]);
	/** Shown while files are dragged over the window. */
	dropTarget = $state(false);
	rotateDialogOpen = $state(false);
	/** The Combine view, while it is open (shown as a tab of its own). */
	combine = $state<CombineState | null>(null);
	/** The Combine view is the tab on screen. */
	combineActive = $state(false);
	/** "Insert pages from file" waiting for its options. */
	insertRequest = $state<InsertRequest | null>(null);
	/** Where pages go when the insert file is still waiting for its password. */
	private pendingInsert: { tabId: number; at: number } | null = null;
	/** The start screen is on screen while documents (or the Combine view) stay open in
	 * their tabs: shown from the title bar's icon and name, left by picking a tab. */
	startShown = $state(false);
	/** The start screen was shown over the Combine view, which going back returns to. */
	private startOverCombine = false;
	private exiting = false;
	/** Tabs on their way to another window. */
	private moving = new Set<number>();

	/** The document on screen; null while the Combine view or the start screen is. */
	active = $derived(
		this.combineActive || this.startShown ? null : (this.tabs.find((t) => t.id === this.activeId) ?? null)
	);

	// ----- notifications and dialogs -----

	notify(toast: Omit<Toast, 'id'>, timeoutMs = 8000) {
		const id = ++toastSeq;
		this.toasts = [...this.toasts, { ...toast, id }];
		if (timeoutMs > 0) setTimeout(() => this.dismissToast(id), timeoutMs);
	}

	dismissToast(id: number) {
		this.toasts = this.toasts.filter((t) => t.id !== id);
	}

	showError(error: AppError, action?: Toast['action']) {
		this.notify({ kind: 'error', message: error.message, suggestion: error.suggestion, action }, 12000);
	}

	/** Shows a modal dialog and resolves with the id of the chosen button. */
	ask(request: Omit<DialogRequest, 'resolve'>): Promise<string> {
		return new Promise((resolve) => {
			this.dialog = {
				...request,
				resolve: (id) => {
					this.dialog = null;
					resolve(id);
				}
			};
		});
	}

	// ----- side panes -----

	restorePanes(panes: PaneLayout) {
		this.leftOpen = panes.leftOpen;
		this.leftWidth = clampWidth(panes.leftWidth, LEFT_LIMITS);
		this.rightOpen = panes.rightOpen;
		this.rightWidth = clampWidth(panes.rightWidth, RIGHT_LIMITS);
		this.panels = normalizeArrangement(panes.leftPanels, panes.rightPanels);
		this.#leftActive = panes.leftActive;
		this.#rightActive = panes.rightActive;
		this.thumbnailWidth = clampThumbWidth(panes.thumbnailWidth);
		this.thumbnailsFit = panes.thumbnailsFit;
		this.thumbnailsGrid = panes.thumbnailsGrid;
		this.recentFilesGrid = panes.recentFilesGrid;
		this.panesRestored = true;
	}

	get panes(): PaneLayout {
		return {
			leftOpen: this.leftOpen,
			leftWidth: this.leftWidth,
			rightOpen: this.rightOpen,
			rightWidth: this.rightWidth,
			leftPanels: [...this.panels.left],
			rightPanels: [...this.panels.right],
			leftActive: this.activePanel('left'),
			rightActive: this.activePanel('right'),
			thumbnailWidth: this.thumbnailWidth,
			thumbnailsFit: this.thumbnailsFit,
			thumbnailsGrid: this.thumbnailsGrid,
			recentFilesGrid: this.recentFilesGrid
		};
	}

	/** Whether a pane shows. Full screen (section 6.1), that is over the pages and for a
	 * while: `leftOpen` and `rightOpen` keep what the window goes back to. */
	isOpen(side: PaneSide): boolean {
		if (fullscreen.on) return this.panels[side].length > 0 && fullscreen.shows(side);
		return side === 'left' ? this.leftOpen : this.rightOpen;
	}

	setOpen(side: PaneSide, open: boolean) {
		if (fullscreen.on) fullscreen.pin(side, open);
		else if (side === 'left') this.leftOpen = open;
		else this.rightOpen = open;
	}

	/** Opens or closes a pane; a pane without panels stays closed. */
	togglePane(side: PaneSide) {
		this.setOpen(side, !this.isOpen(side) && this.panels[side].length > 0);
	}

	/** The panel a pane shows: the one last picked there, or its first. */
	activePanel(side: PaneSide): PanelId | null {
		const active = side === 'left' ? this.#leftActive : this.#rightActive;
		const list = this.panels[side];
		return active !== null && list.includes(active) ? active : (list[0] ?? null);
	}

	setActivePanel(side: PaneSide, panel: PanelId) {
		if (side === 'left') this.#leftActive = panel;
		else this.#rightActive = panel;
	}

	/** The panel is on screen: its pane is open and shows it. */
	isShown(panel: PanelId): boolean {
		const side = paneOf(this.panels, panel);
		return this.isOpen(side) && this.activePanel(side) === panel;
	}

	/** Opens the pane that holds a panel, showing it. */
	showPanel(panel: PanelId) {
		const side = paneOf(this.panels, panel);
		this.setActivePanel(side, panel);
		this.setOpen(side, true);
	}

	/**
	 * Moves a panel to `index` in a pane's row. Moved into the other pane, it is shown
	 * there, and a pane left without panels closes.
	 */
	movePanel(panel: PanelId, side: PaneSide, index: number) {
		const from = paneOf(this.panels, panel);
		this.panels = movePanel(this.panels, panel, side, index);
		if (from === side) return;
		if (this.panels[from].length === 0) {
			this.setOpen(from, false);
			// Full screen, that hid it for now only.
			if (from === 'left') this.leftOpen = false;
			else this.rightOpen = false;
		}
		this.setActivePanel(side, panel);
		this.setOpen(side, true);
	}

	// ----- opening -----

	/** Reads the recent files again after something here may have changed them, and tells
	 * Lectrix's other windows to. */
	async refreshRecent() {
		await this.loadRecent();
		void recentChanged().catch(() => {});
	}

	/** Reads the recent files (they may have changed in another window). */
	async loadRecent() {
		try {
			this.recent = await listRecentFiles();
		} catch {
			this.recent = [];
		}
	}

	handleOpenResults(results: OpenResult[]) {
		let activate: number | null = null;
		for (const result of results) {
			switch (result.kind) {
				case 'opened': {
					// A document can come back twice at startup: restored after a crash, then
					// listed again among the documents Rust has open.
					if (!this.tabs.some((t) => t.id === result.document.id)) {
						this.tabs = [...this.tabs, new DocTab(result.document, this.settings?.scrollMode)];
					}
					activate = result.document.id;
					break;
				}
				case 'alreadyOpen':
					// In a tab here. (In another window, Rust shows it there.)
					if (this.tabs.some((t) => t.id === result.id)) activate = result.id;
					break;
				case 'needsPassword':
					this.askPassword(result.token, result.name, result.retry, 'open');
					break;
				case 'failed':
					this.notify(
						{
							kind: 'error',
							message: `${result.name}: ${result.error.message}`,
							suggestion: result.error.suggestion
						},
						12000
					);
					break;
			}
		}
		if (activate !== null) this.activate(activate);
		void this.refreshRecent();
	}

	async open() {
		try {
			this.handleOpenResults(await openWithDialog());
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	async openRecent(index: number) {
		try {
			const entry = this.recent.find((r) => r.index === index);
			if (entry && !entry.exists) {
				this.notify({
					kind: 'error',
					message: `${entry.name} is no longer in ${entry.folder}.`,
					suggestion: 'It was moved or deleted. It has been removed from the list.'
				});
				await removeRecent(index);
				await this.refreshRecent();
				return;
			}
			this.handleOpenResults([await openRecent(index)]);
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	async openStartup() {
		try {
			// The documents Rust has for this window come first: the tab this window was made
			// for, or the ones it had before its page reloaded (a renderer crash, or a test
			// driver navigating it).
			await this.adoptTabs();
			this.handleOpenResults(await openStartupDocuments());
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	/**
	 * Adds the tabs Rust has for this window that it does not show yet (section 8): a tab
	 * moved here from another window comes with its page display, and opens where the user
	 * was. The last one is shown.
	 */
	async adoptTabs() {
		let shown: number | null = null;
		for (const { document, handoff } of await listOpenDocuments()) {
			if (this.tabs.some((t) => t.id === document.id)) continue;
			this.tabs = [...this.tabs, new DocTab(document, this.settings?.scrollMode, handoff)];
			shown = document.id;
		}
		if (shown !== null) this.activate(shown);
	}

	/** Tabs were moved into this window while it was open. */
	async tabsMovedIn() {
		try {
			await this.adoptTabs();
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	/**
	 * Offers to restore unsaved changes a crash left behind (section 7). "Not now" keeps
	 * them for the next start.
	 */
	async offerRecovery() {
		let found;
		try {
			found = await listRecovered();
		} catch {
			return;
		}
		if (found.length === 0) return;
		const slots = found.map((d) => d.slot);
		const choice = await this.ask(recoveryQuestion(found));
		try {
			if (choice === 'restore') {
				const results = await restoreRecovered(slots);
				this.handleOpenResults(results);
				const restored = results.flatMap((r) => (r.kind === 'opened' ? [r.document.name] : []));
				if (restored.length > 0) {
					this.notify(
						{
							kind: 'info',
							message:
								restored.length === 1
									? `${restored[0]} is back with its unsaved changes.`
									: `${restored.length} documents are back with their unsaved changes.`,
							suggestion: 'Save to keep the changes.'
						},
						10000
					);
				}
			} else if (choice === 'discard' && (await this.ask(discardQuestion(found.length))) === 'discard') {
				await discardRecovered(slots);
			}
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	askPassword(token: number, name: string, retry: boolean, purpose: PasswordRequest['purpose']) {
		this.passwordPrompts = [...this.passwordPrompts, { token, name, retry, purpose }];
	}

	async submitPassword(token: number, password: string | null) {
		const prompt = this.passwordPrompts.find((p) => p.token === token);
		this.passwordPrompts = this.passwordPrompts.filter((p) => p.token !== token);
		if (password === null) {
			await cancelUnlock(token);
			if (prompt?.purpose === 'insert') this.pendingInsert = null;
			return;
		}
		try {
			const result = await unlockDocument(token, password);
			if (prompt?.purpose === 'combine') {
				if (this.combine) this.combine.addOpened([result]);
				// The Combine view closed while the password was asked for.
				else if (result.kind === 'opened') void closeDocument(result.document.id).catch(() => {});
			}
			else if (prompt?.purpose === 'insert') this.insertSourceOpened(result);
			else this.handleOpenResults([result]);
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	// ----- tabs -----

	activate(id: number) {
		if (!this.tabs.some((t) => t.id === id)) return;
		this.startShown = false;
		if (this.combineActive) this.leaveCombine();
		if (this.activeId === id) return;
		const previous = this.active;
		if (previous) this.rememberView(previous);
		this.activeId = id;
	}

	// ----- combining files (section 6.4) -----

	/** Opens (or shows) the Combine view. */
	openCombine() {
		const current = this.active;
		if (current) this.rememberView(current);
		if (!this.combine) {
			this.combine = new CombineState({
				notify: (toast, timeoutMs) => this.notify(toast, timeoutMs),
				showError: (error) => this.showError(error),
				ask: (request) => this.ask(request),
				askPassword: (token, name, retry) => this.askPassword(token, name, retry, 'combine'),
				saveTab: async (id) => {
					const tab = this.tabs.find((t) => t.id === id);
					if (tab) return (await this.save(tab)) !== null;
					// In a tab of another window: saved from here, and Rust tells that window.
					try {
						await saveCommand(id);
						return true;
					} catch (e) {
						const error = toAppError(e);
						// Closed meanwhile: nothing left to save.
						if (error.code === 'documentClosed') return true;
						this.showError(error);
						return false;
					}
				},
				opened: (result) => this.handleOpenResults([result])
			});
		}
		this.startShown = false;
		this.combineActive = true;
		void setDropTarget(true).catch(() => {});
	}

	private leaveCombine() {
		this.combineActive = false;
		void setDropTarget(false).catch(() => {});
	}

	/** Closes the Combine view, after asking if pages were arranged and not combined. */
	async closeCombine() {
		const combine = this.combine;
		if (!combine) return;
		if (combine.running) {
			this.notify({ kind: 'info', message: 'Lectrix is combining files.', suggestion: 'Stop it first, or wait for it to finish.' });
			return;
		}
		if (combine.pages.length > 0 && !combine.combinedAs) {
			const choice = await this.ask({
				title: 'Close Combine files?',
				message: 'The pages you arranged will be lost.',
				detail: 'The files themselves are not changed.',
				buttons: [
					{ id: 'close', label: 'Close', primary: true },
					{ id: 'cancel', label: 'Cancel' }
				],
				cancel: 'cancel'
			});
			if (choice !== 'close') return;
		}
		this.leaveCombine();
		this.combine = null;
		this.endStartIfAlone();
		await combine.dispose();
	}

	// ----- the start screen over open documents (section 6.1) -----

	/**
	 * The title bar's icon and name: shows the start screen while documents (or the Combine
	 * view) stay open in their tabs, or goes back from it to what was on screen. With
	 * nothing open the start screen is there already.
	 */
	toggleStart() {
		if (this.startShown) {
			this.startShown = false;
			// (Or to the Combine view when the documents were closed meanwhile.)
			if ((this.startOverCombine || this.tabs.length === 0) && this.combine) this.openCombine();
			return;
		}
		const current = this.active;
		if (!current && !this.combineActive) return;
		if (current) this.rememberView(current);
		this.startOverCombine = this.combineActive;
		if (this.combineActive) this.leaveCombine();
		this.startShown = true;
	}

	/** With the last tab closed, the start screen is simply what shows. */
	private endStartIfAlone() {
		if (this.tabs.length === 0 && !this.combine) this.startShown = false;
	}

	// ----- inserting pages from a file (section 6.4) -----

	/** Asks for a file and then for the options; pages go before page `at` by default. */
	async insertFromFile(tab: DocTab, at = tab.currentPage + 1) {
		if (this.refuseReadOnly(tab)) return;
		if (!tab.flags.canAssemble) {
			this.notify({
				kind: 'error',
				message: 'This document’s security settings don’t allow inserting pages.',
				suggestion: 'Combine it with the other file into a new one instead (File > Combine files).'
			});
			return;
		}
		this.pendingInsert = { tabId: tab.id, at: Math.min(at, tab.pageCount) };
		try {
			const result = await openInsertSource();
			if (result) this.insertSourceOpened(result);
			else this.pendingInsert = null;
		} catch (e) {
			this.pendingInsert = null;
			this.showError(toAppError(e));
		}
	}

	private insertSourceOpened(result: OpenResult) {
		const pending = this.pendingInsert;
		if (result.kind === 'needsPassword') {
			this.askPassword(result.token, result.name, result.retry, 'insert');
			return;
		}
		this.pendingInsert = null;
		if (result.kind === 'failed') {
			this.notify(
				{ kind: 'error', message: `${result.name}: ${result.error.message}`, suggestion: result.error.suggestion },
				12000
			);
			return;
		}
		if (result.kind !== 'opened' || !pending) return;
		const source = result.document;
		if (!source.flags.canCopy) {
			void closeDocument(source.id).catch(() => {});
			this.notify(
				{
					kind: 'error',
					message: `${source.name} doesn’t allow copying its pages into another document.`,
					suggestion: 'Its security settings forbid it. Ask its author for an unrestricted copy.'
				},
				12000
			);
			return;
		}
		this.insertRequest = { tabId: pending.tabId, source, at: pending.at };
	}

	/** Inserts the chosen pages (all if empty) before page `at`. */
	async insertPages(
		pages: number[],
		at: number,
		bookmarks: BookmarkMode,
		labels: InsertLabelMode
	): Promise<boolean> {
		const request = this.insertRequest;
		const tab = request && this.tabs.find((t) => t.id === request.tabId);
		if (!request || !tab) {
			this.cancelInsert();
			return false;
		}
		if (request.source.flags.signed && (await this.ask(signedWarning([request.source.name], true))) !== 'go') {
			return false;
		}
		const change = await this.apply(tab, {
			kind: 'insertPages',
			source: request.source.id,
			pages,
			at,
			bookmarks,
			labels
		});
		if (!change) return false;
		this.cancelInsert();
		this.activate(tab.id);
		tab.viewer?.goTo({ page: at, offset: 0 });
		const inserted = change.mergeReport?.pages ?? pages.length;
		const detail = change.mergeReport ? reportDetail(change.mergeReport) : null;
		this.notify(
			{
				kind: 'info',
				message: `Inserted ${inserted === 1 ? '1 page' : `${inserted.toLocaleString()} pages`} from ${request.source.name}.`,
				suggestion: detail
			},
			detail ? 15000 : 6000
		);
		return true;
	}

	/** Closes the insert dialog and the file it would have taken pages from. */
	cancelInsert() {
		const request = this.insertRequest;
		this.insertRequest = null;
		if (request) void closeDocument(request.source.id).catch(() => {});
	}

	cycleTab(direction: 1 | -1) {
		// From the start screen, back to the document it was shown over.
		if (this.startShown && this.activeId !== null) {
			this.activate(this.activeId);
			return;
		}
		if (this.tabs.length < 2 || this.activeId === null) return;
		const i = this.tabs.findIndex((t) => t.id === this.activeId);
		const next = this.tabs[(i + direction + this.tabs.length) % this.tabs.length];
		if (next) this.activate(next.id);
	}

	moveTab(from: number, to: number) {
		if (from === to || from < 0 || to < 0 || from >= this.tabs.length || to >= this.tabs.length) return;
		const tabs = [...this.tabs];
		const [tab] = tabs.splice(from, 1);
		if (tab) tabs.splice(to, 0, tab);
		this.tabs = tabs;
	}

	// ----- windows (section 8) -----

	/** The tab is all this window shows: in a new window it would be this window again. */
	isOnlyTab(id: number): boolean {
		return this.tabs.length === 1 && this.tabs[0]?.id === id && !this.combine;
	}

	/**
	 * Moves a tab into another window: a new one, one that is open, or wherever it was
	 * dropped. The document stays open in Rust as it is, unsaved changes and undo history
	 * included; the tab takes its place in the document and its page display along. A
	 * window left with nothing to show closes. Returns whether the tab moved.
	 */
	async moveToWindow(id: number, target: MoveTarget): Promise<boolean> {
		const tab = this.tabs.find((t) => t.id === id);
		// Not in the middle of a save, whose result this window's tab is waiting for.
		if (!tab || tab.saving || this.moving.has(id)) return false;
		this.moving.add(id);
		try {
			if (this.insertRequest?.tabId === id) this.cancelInsert();
			await this.settled();
			// Where the user is: the tab opens there in its new window.
			await rememberView(tab.id, tab.viewState()).catch(() => {});
			if ((await moveDocument(id, target, tab.handoff())) === null) return false;
			this.removeTab(tab);
			if (this.tabs.length === 0 && !this.combine) void getCurrentWindow().close();
			return true;
		} catch (e) {
			this.showError(toAppError(e));
			return false;
		} finally {
			this.moving.delete(id);
		}
	}

	/** A file opened in another window is open in a tab of this one: shows it. */
	showDocument(id: number) {
		this.activate(id);
	}

	/**
	 * Closes Lectrix's other windows, each after asking about its unsaved changes, as its
	 * close button does. Resolves once they are gone, or with false as soon as one stays
	 * open (the user cancelled there).
	 */
	async closeOtherWindows(): Promise<boolean> {
		if ((await listWindows()).length === 0) return true;
		let declined = false;
		const unlisten = await onCloseDeclined(() => (declined = true));
		try {
			await closeOtherWindowsCommand();
			for (;;) {
				await new Promise((r) => setTimeout(r, 100));
				if ((await listWindows()).length === 0) return true;
				if (declined) return false;
			}
		} finally {
			unlisten();
		}
	}

	/** File > Exit: closes every window, this one last. */
	async exit() {
		try {
			if (await this.closeOtherWindows()) await getCurrentWindow().close();
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	/** Takes a tab out of this window: the one after it shows, or what else is open. */
	private removeTab(tab: DocTab) {
		tab.search.cancel();
		const index = this.tabs.indexOf(tab);
		this.tabs = this.tabs.filter((t) => t !== tab);
		if (this.activeId === tab.id) {
			const next = this.tabs[Math.min(index, this.tabs.length - 1)];
			this.activeId = next?.id ?? null;
			// The last document gone while the Combine view is open: show it.
			if (!next && this.combine && !this.startShown) this.openCombine();
		}
		this.endStartIfAlone();
	}

	rememberView(tab: DocTab) {
		// Best effort: a lost view position is not worth an error message.
		rememberView(tab.id, tab.viewState()).catch(() => {});
	}

	/** Asks about unsaved changes. Returns false if the user cancelled. */
	private async confirmDiscard(tab: DocTab): Promise<boolean> {
		if (!tab.state.dirty) return true;
		const choice = await this.ask({
			title: 'Save changes?',
			message: `Do you want to save the changes to ${tab.name}?`,
			detail: 'If you don’t save, your changes will be lost.',
			buttons: [
				{ id: 'save', label: 'Save', primary: true },
				{ id: 'discard', label: 'Don’t save' },
				{ id: 'cancel', label: 'Cancel' }
			],
			cancel: 'cancel'
		});
		if (choice === 'cancel') return false;
		if (choice === 'save') return (await this.save(tab)) !== null;
		return true;
	}

	async closeTab(id: number) {
		const tab = this.tabs.find((t) => t.id === id);
		if (!tab) return;
		// From the start screen, a tab closes without coming on screen, unless it has
		// changes to ask about.
		if (!this.startShown) this.activate(id);
		if (this.insertRequest?.tabId === id) this.cancelInsert();
		await this.settled();
		if (tab.state.dirty) this.activate(id);
		if (!(await this.confirmDiscard(tab))) return;
		// Moved to another window while it asked: it is that window's to close now.
		if (!this.tabs.includes(tab)) return;
		this.rememberView(tab);
		this.removeTab(tab);
		await closeDocument(id).catch(() => {});
		void this.refreshRecent();
	}

	/**
	 * Handles the window's close request: prompts about its unsaved documents. Returns true
	 * if the window may close. Lectrix quits with its last window; while others stay open,
	 * Rust closes this window's documents once it is gone.
	 */
	async confirmExit(): Promise<boolean> {
		if (this.exiting) return true;
		if (this.combine?.running) {
			// Closing now would leave the unfinished file behind; stop the merge first.
			const choice = await this.ask({
				title: 'Lectrix is combining files',
				message: 'Stop combining and close Lectrix?',
				detail: 'No file is written when combining is stopped.',
				buttons: [
					{ id: 'stop', label: 'Stop and close', primary: true },
					{ id: 'cancel', label: 'Keep combining' }
				],
				cancel: 'cancel'
			});
			if (choice !== 'stop') return false;
			const combine = this.combine;
			combine.stop();
			while (combine.running) await new Promise((r) => setTimeout(r, 50));
		}
		await this.settled();
		const dirty = this.tabs.filter((t) => t.state.dirty);
		if (dirty.length === 1) {
			if (!(await this.confirmDiscard(dirty[0]!))) return false;
		} else if (dirty.length > 1) {
			const choice = await this.ask({
				title: 'Save changes?',
				message: `${dirty.length} documents have unsaved changes.`,
				detail: dirty.map((t) => t.name).join(', '),
				buttons: [
					{ id: 'save', label: 'Save all', primary: true },
					{ id: 'discard', label: 'Don’t save' },
					{ id: 'cancel', label: 'Cancel' }
				],
				cancel: 'cancel'
			});
			if (choice === 'cancel') return false;
			if (choice === 'save') {
				for (const tab of dirty) {
					this.activate(tab.id);
					if ((await this.save(tab)) === null) return false;
				}
			}
		}
		for (const tab of this.tabs) this.rememberView(tab);
		await exitConfirmed().catch(() => {});
		this.exiting = true;
		return true;
	}

	/**
	 * Asks about unsaved documents as closing the windows does (the other windows first,
	 * which close; then this one), then runs `quit`, which ends the app (restarting into an
	 * update). Returns false if the user cancelled. If `quit` fails, Lectrix stays open and
	 * closing asks again.
	 */
	async quitWith(quit: () => Promise<void>): Promise<boolean> {
		if (!(await this.closeOtherWindows())) return false;
		if (!(await this.confirmExit())) return false;
		try {
			await quit();
		} catch (e) {
			this.exiting = false;
			throw e;
		}
		return true;
	}

	// ----- editing -----

	/** Warns once before the first edit of a signed document (section 5.4). */
	async allowEdit(tab: DocTab): Promise<boolean> {
		if (!tab.flags.signed || tab.signedWarningAccepted) return true;
		const choice = await this.ask({
			title: 'This document is signed',
			message: 'Changes you make will show as edits made after it was signed.',
			detail:
				'The signature stays valid for the signed version, and Lectrix saves your changes after it without altering the signed part.',
			buttons: [
				{ id: 'edit', label: 'Edit anyway', primary: true },
				{ id: 'cancel', label: 'Cancel' }
			],
			cancel: 'cancel'
		});
		tab.signedWarningAccepted = choice === 'edit';
		return tab.signedWarningAccepted;
	}

	/**
	 * Tells the user why nothing happens in a document open for reading only (a file that
	 * claims PDF/A, section 5.4), and offers to enable editing. Returns true if it is.
	 */
	refuseReadOnly(tab: DocTab): boolean {
		if (!tab.readOnly) return false;
		const message = `${tab.name} is open for reading only, because it is a PDF/A file.`;
		// Said once, however often the document is clicked with a tool meanwhile.
		if (this.toasts.some((t) => t.message === message)) return true;
		this.notify({
			kind: 'info',
			message,
			suggestion: 'Enable editing to change it.',
			action: { label: 'Enable editing', run: () => void this.enableEditing(tab) }
		});
		return true;
	}

	/** Lets a document that is open for reading only be changed, in Rust first. */
	async enableEditing(tab: DocTab) {
		if (!tab.readOnly) return;
		try {
			await enableEditingCommand(tab.id);
			tab.readOnly = false;
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	/**
	 * Says that saving took the PDF/A claim out of a file (section 5.4): it can't be put
	 * back, so the reason and what would have kept it are part of the message.
	 */
	notifyPdfaRemoved(name: string, level: PdfaLevel) {
		this.notify(
			{
				kind: 'info',
				message: `${name} no longer claims to be ${pdfaName(level)}.`,
				suggestion:
					'Annotations, and pages inserted or deleted, may leave a file that no longer complies with that standard, so Lectrix removed the claim when saving. Changes to bookmarks, page labels, page rotation and the order of the pages alone keep it.'
			},
			20000
		);
	}

	/** Edits on their way to Rust; saving and closing wait for them. */
	private inFlight = new Set<Promise<unknown>>();

	/** Resolves once every edit started so far has finished (or failed). */
	async settled() {
		while (this.inFlight.size > 0) await Promise.allSettled([...this.inFlight]);
	}

	/** Applies an operation. Returns what changed, or null if it was cancelled or failed. */
	apply(tab: DocTab, operation: OperationInput): Promise<DocumentChange | null> {
		const edit = this.applyNow(tab, operation);
		this.inFlight.add(edit);
		const done = () => void this.inFlight.delete(edit);
		edit.then(done, done);
		return edit;
	}

	private async applyNow(tab: DocTab, operation: OperationInput): Promise<DocumentChange | null> {
		if (!(await this.allowEdit(tab))) return null;
		try {
			const change = await applyOperation(tab.id, operation);
			tab.applyChange(change);
			return change;
		} catch (e) {
			this.showError(toAppError(e));
			return null;
		}
	}

	async rotatePages(tab: DocTab, pages: number[], degrees: number) {
		if (this.refuseReadOnly(tab)) return;
		if (!tab.flags.canAssemble) {
			this.notify({
				kind: 'error',
				message: 'This document’s security settings don’t allow rotating pages.',
				suggestion: 'Rotate the view instead (View > Rotate view); it doesn’t change the file.'
			});
			return;
		}
		await this.apply(tab, { kind: 'rotatePages', pages, degrees });
	}

	async undo(tab: DocTab) {
		if (!tab.state.undoName) return;
		try {
			tab.applyChange(await undoCommand(tab.id));
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	async redo(tab: DocTab) {
		if (!tab.state.redoName) return;
		try {
			tab.applyChange(await redoCommand(tab.id));
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	// ----- saving -----

	private afterSave(tab: DocTab, result: SaveResult) {
		tab.state = result.state;
		tab.applySaved(result);
		tab.name = result.name;
		tab.path = result.path;
		tab.banner = null;
		if (result.pdfaRemoved) this.notifyPdfaRemoved(tab.name, result.pdfaRemoved);
		if (result.fellBackToFull && tab.fileFlags.repaired) {
			tab.fileFlags = { ...tab.fileFlags, repaired: false };
			this.notify(
				{
					kind: 'info',
					message: `${tab.name} was damaged, so Lectrix saved a complete, repaired copy instead of adding your changes to the end of the file.`,
					suggestion: 'Nothing else changed. The file now opens normally in other apps.'
				},
				15000
			);
		}
	}

	/** Saves in place. Returns null if the save failed or was cancelled. */
	async save(tab: DocTab): Promise<SaveResult | null> {
		if (tab.saving) return null;
		tab.saving = true;
		await this.settled();
		try {
			const result = await saveCommand(tab.id);
			this.afterSave(tab, result);
			return result;
		} catch (e) {
			const error = toAppError(e);
			if (error.code === 'fileLocked') {
				this.showError(error, { label: 'Save As…', run: () => void this.saveAs(tab) });
			} else {
				this.showError(error);
			}
			return null;
		} finally {
			tab.saving = false;
		}
	}

	async saveAs(tab: DocTab, optimized = false): Promise<SaveResult | null> {
		if (tab.saving) return null;
		tab.saving = true;
		await this.settled();
		try {
			const result = await saveAsCommand(tab.id, optimized);
			if (result) {
				this.afterSave(tab, result);
				void this.refreshRecent();
			}
			return result;
		} catch (e) {
			this.showError(toAppError(e));
			return null;
		} finally {
			tab.saving = false;
		}
	}

	/** A document in a tab here was saved from another window (its Combine view). */
	savedElsewhere(id: number, result: SaveResult) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) this.afterSave(tab, result);
	}

	// ----- outside changes -----

	fileChanged(id: number, exists: boolean) {
		const tab = this.tabs.find((t) => t.id === id);
		if (!tab) return;
		tab.banner = !exists ? 'deletedOnDisk' : tab.state.dirty ? 'changedOnDiskDirty' : 'changedOnDisk';
	}

	async reload(tab: DocTab) {
		try {
			const info = await reloadDocument(tab.id);
			const position = tab.viewer?.position();
			tab.update(info);
			tab.banner = null;
			if (position) tab.viewer?.goTo(position, { recordHistory: false });
			if (tab.search.query) void tab.search.start(tab.search.query, tab.currentPage);
		} catch (e) {
			this.showError(toAppError(e));
		}
	}
}

export const app = new AppStore();
