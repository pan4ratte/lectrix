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
	openInsertSources,
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
	type MergeReport,
	type MoveTarget,
	type OpenResult,
	type OperationInput,
	type PaneLayout,
	type PanelId,
	type PdfaLevel,
	type CompressStep,
	type PendingProtection,
	type ProtectionInfo,
	type ProtectionInput,
	type RecentFile,
	type UpdateInfo,
	type SaveResult,
	type Settings,
	type StartupInfo
} from '#lib/ipc/index.ts';
import {
	COMBINE_LIMITS,
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
import { displayShortcut, shortcutOf } from '#lib/shortcuts.ts';

import { MAX_PANES } from '#lib/features/split/split.ts';

import { DocTab } from './doc.svelte.ts';
import { TabGroup } from './group.svelte.ts';

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
	/** What the file is opened for: a tab, the Combine view, inserting its pages, removing
	 * its password (Unlock), changing its protection (Protect) or compressing it. */
	purpose: 'open' | 'combine' | 'insert' | 'unlock' | 'protect' | 'compress';
	/** Takes the password (null for Cancel) in place of opening the file: Unlock's,
	 * Protect's and Compress's. */
	answer?: (password: string | null) => Promise<void>;
}

/** The Compress dialog (section 6.11), for a file Rust holds under `token`. */
export type CompressRequest = Extract<CompressStep, { kind: 'ready' }>;

/** A tab of a document's properties (section 6.10). */
export type PropertiesSection = 'description' | 'security' | 'fonts';

/** The document's properties, open for the document in tab `tabId`. */
export interface PropertiesRequest {
	tabId: number;
	section: PropertiesSection;
}

/** The Print dialog (section 6.12), for the document in tab `tabId`. */
export interface PrintRequest {
	tabId: number;
}

/** The Protect dialog (section 6.9), for a file or for a document in a tab. */
export interface ProtectRequest {
	name: string;
	/** The protection the file has now, which the dialog starts from. */
	protection: ProtectionInfo | null;
	/** A change of the document's protection not saved yet, which it starts from instead. */
	pending: PendingProtection | null;
	/** Say that its signatures will not be valid (a document: nothing is asked after). */
	signedWarning: boolean;
	/** It has protection to remove (a document protected, or with a change waiting). */
	canRemove: boolean;
	/** The protection chosen, 'remove', or null for Cancel. */
	resolve: (choice: ProtectionInput | 'remove' | null) => void;
}

/** "Insert pages from file": the files chosen, waiting for the dialog's options. */
export interface InsertRequest {
	tabId: number;
	/** In the order their pages go in. */
	sources: DocumentInfo[];
	/** The pages go before this page (0-based; the page count appends). */
	at: number;
	/** Run when the insertion is given up: what was started for it is ended. */
	back?: () => void;
}

/** The Page labels dialog, open for a document whose pages are being organized (section
 * 6.3). */
export interface LabelsRequest {
	tabId: number;
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
	/** The tabs of the window's row: each a view of one document, or views joined side by
	 * side (section 6.1, split view). */
	groups = $state<TabGroup[]>([]);
	/** Every view in the window, tab by tab (a document shown twice is in it twice). */
	tabs = $derived(this.groups.flatMap((g) => g.views));
	/** The tab shown, or the one the start screen or the Combine view is shown over. */
	activeKey = $state<number | null>(null);
	activeGroup = $derived(this.groups.find((g) => g.key === this.activeKey) ?? null);
	/** The document of the pane in focus in that tab. */
	activeId = $derived(this.activeGroup?.focused?.id ?? null);
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
	/** The width of the Combine view's pane (its files and options). */
	combineWidth = $state(COMBINE_LIMITS.initial);
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

	/** A command's first shortcut as Settings has it, shown as the platform shows shortcuts
	 * (⇧⌘S on macOS), for menus and tooltips; '' without one. */
	shortcut(command: CommandName): string {
		return displayShortcut(shortcutOf(command, this.settings?.shortcuts));
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
	/** A dialog is open. The system's menu bar on macOS, which a dialog does not cover as it
	 * covers the title bar's menus, offers nothing that acts on the window meanwhile. */
	get modalOpen(): boolean {
		return (
			this.dialog !== null ||
			this.passwordPrompts.length > 0 ||
			this.settingsOpen ||
			this.aboutOpen ||
			this.whatsNewOpen ||
			this.annotationPropertiesOpen ||
			this.labelsRequest !== null ||
			this.insertRequest !== null ||
			this.protectRequest !== null ||
			this.compressRequest !== null ||
			this.propertiesRequest !== null ||
			this.printRequest !== null
		);
	}
	toasts = $state<Toast[]>([]);
	/** Shown while files are dragged over the window. */
	dropTarget = $state(false);
	/** The tabs Rust has for this window are being taken in (`adoptTabs`). */
	private adopting: Promise<void> = Promise.resolve();
	/** The Page labels dialog is open, over the grid of a document's pages (section 6.3). */
	labelsRequest = $state<LabelsRequest | null>(null);
	/** The Protect dialog is open (section 6.9). */
	protectRequest = $state<ProtectRequest | null>(null);
	/** The Compress dialog is open (section 6.11). */
	compressRequest = $state<CompressRequest | null>(null);
	/** A document's properties are open (section 6.10). */
	propertiesRequest = $state<PropertiesRequest | null>(null);
	/** The Print dialog is open (section 6.12). */
	printRequest = $state<PrintRequest | null>(null);
	/** The Combine view, while it is open (shown as a tab of its own). */
	combine = $state<CombineState | null>(null);
	/** The Combine view is the tab on screen. */
	combineActive = $state(false);
	/** "Insert pages from file" waiting for its options. */
	insertRequest = $state<InsertRequest | null>(null);
	/** Where pages go when the insert file is still waiting for its password. */
	/** An insertion whose files are being chosen or opened: each file's place in their
	 * order (null until it is open, or if it could not be), and the password prompts
	 * waited for, each with the place of its file. */
	private pendingInsert: {
		tabId: number;
		at: number;
		back?: () => void;
		slots: (DocumentInfo | null)[];
		waiting: Map<number, number>;
	} | null = null;
	/** The start screen is on screen while documents (or the Combine view) stay open in
	 * their tabs: shown from the title bar's icon and name, left by picking a tab. */
	startShown = $state(false);
	/** The start screen was shown over the Combine view, which going back returns to. */
	private startOverCombine = false;
	private exiting = false;
	/** Tabs on their way to another window. */
	private moving = new Set<number>();

	/** The document on screen (the pane in focus, in a joined tab); null while the Combine
	 * view or the start screen is. */
	active = $derived(this.combineActive || this.startShown ? null : (this.activeGroup?.focused ?? null));

	/** One view of each document in the window. */
	get documents(): DocTab[] {
		const seen = new Set<number>();
		return this.tabs.filter((t) => !seen.has(t.id) && seen.add(t.id));
	}

	/** The tab `view` is in. */
	groupOf(view: DocTab): TabGroup | null {
		return this.groups.find((g) => g.has(view)) ?? null;
	}

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

	/** Shows the Protect dialog and resolves with what was chosen (section 6.9). */
	askProtection(request: Omit<ProtectRequest, 'resolve'>): Promise<ProtectionInput | 'remove' | null> {
		return new Promise((resolve) => {
			this.protectRequest = { ...request, resolve };
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
		this.combineWidth = clampWidth(panes.combineWidth, COMBINE_LIMITS);
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
			recentFilesGrid: this.recentFilesGrid,
			combineWidth: this.combineWidth
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

	/** Shows a panel, or, while it is on screen, closes the pane that shows it. */
	togglePanel(panel: PanelId) {
		if (this.isShown(panel)) this.setOpen(paneOf(this.panels, panel), false);
		else this.showPanel(panel);
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

	/** Takes in what opening files gave: tabs, password prompts, errors. Returns the id of
	 * the tab it shows, if it shows one. */
	handleOpenResults(results: OpenResult[]): number | null {
		let activate: number | null = null;
		for (const result of results) {
			switch (result.kind) {
				case 'opened': {
					// A document can come back twice at startup: restored after a crash, then
					// listed again among the documents Rust has open.
					if (!this.tabs.some((t) => t.id === result.document.id)) {
						this.groups = [...this.groups, new TabGroup([new DocTab(result.document, this.settings?.scrollMode)])];
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
		return activate;
	}

	/** Asks for files and opens them. Returns the tab shown for them, if one is. */
	async open(): Promise<DocTab | null> {
		try {
			const shown = this.handleOpenResults(await openWithDialog());
			return this.tabs.find((t) => t.id === shown) ?? null;
		} catch (e) {
			this.showError(toAppError(e));
			return null;
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
	adoptTabs(): Promise<void> {
		const adopted = (async () => {
			let shown: number | null = null;
			for (const { document, handoff } of await listOpenDocuments()) {
				if (this.tabs.some((t) => t.id === document.id)) continue;
				this.groups = [...this.groups, new TabGroup([new DocTab(document, this.settings?.scrollMode, handoff)])];
				shown = document.id;
			}
			if (shown !== null) this.activate(shown);
		})();
		this.adopting = adopted.catch(() => {});
		return adopted;
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

	askPassword(
		token: number,
		name: string,
		retry: boolean,
		purpose: PasswordRequest['purpose'],
		answer?: PasswordRequest['answer']
	) {
		this.passwordPrompts = [...this.passwordPrompts, { token, name, retry, purpose, answer }];
	}

	async submitPassword(token: number, password: string | null) {
		const prompt = this.passwordPrompts.find((p) => p.token === token);
		this.passwordPrompts = this.passwordPrompts.filter((p) => p.token !== token);
		if (prompt?.answer) {
			await prompt.answer(password);
			return;
		}
		if (password === null) {
			await cancelUnlock(token);
			if (prompt?.purpose === 'insert') this.insertPasswordDone(token, null);
			return;
		}
		try {
			const result = await unlockDocument(token, password);
			if (prompt?.purpose === 'combine') {
				if (this.combine) this.combine.addOpened([result]);
				// The Combine view closed while the password was asked for.
				else if (result.kind === 'opened') void closeDocument(result.document.id).catch(() => {});
			}
			else if (prompt?.purpose === 'insert') this.insertPasswordDone(token, result);
			else this.handleOpenResults([result]);
		} catch (e) {
			if (prompt?.purpose === 'insert') this.insertPasswordDone(token, null);
			this.showError(toAppError(e));
		}
	}

	// ----- tabs -----

	/** Shows the tab of document `id`, with the focus on its pane (the one in the tab shown,
	 * if that tab has it). */
	activate(id: number) {
		const holds = (g: TabGroup) => g.views.some((v) => v.id === id);
		const group = this.activeGroup && holds(this.activeGroup) ? this.activeGroup : this.groups.find(holds);
		if (!group) return;
		const view = group.focused?.id === id ? group.focused : (group.views.find((v) => v.id === id) ?? null);
		this.show(group, view);
	}

	/** Shows `group`, with the focus on `view` among its panes if given. */
	show(group: TabGroup, view: DocTab | null = null) {
		if (!this.groups.includes(group)) return;
		this.startShown = false;
		if (this.combineActive) this.leaveCombine();
		if (this.activeKey === group.key && (!view || group.focused === view)) return;
		const previous = this.active;
		if (previous) this.rememberView(previous);
		this.activeKey = group.key;
		this.shownOrder = [group.key, ...this.shownOrder.filter((k) => k !== group.key)];
		if (view) group.focusOn(view);
	}

	/** The tabs as they were last shown, the latest first (keys). */
	private shownOrder: number[] = [];

	/** The tab shown before `group` was, of those still open: the one it goes beside when it
	 * is dragged to an edge of the pages while on screen itself. Else its neighbour in the
	 * row; null when it is the only tab. */
	shownBefore(group: TabGroup): TabGroup | null {
		for (const key of this.shownOrder) {
			const found = key === group.key ? null : this.groups.find((g) => g.key === key);
			if (found) return found;
		}
		const i = this.groups.indexOf(group);
		return this.groups[i - 1] ?? this.groups[i + 1] ?? null;
	}

	// ----- documents side by side (section 6.1, split view) -----

	/** Puts the focus on a pane: a click in it, or on its header. */
	focusView(view: DocTab) {
		const group = this.groupOf(view);
		if (!group) return;
		if (group !== this.activeGroup || this.startShown || this.combineActive) this.show(group, view);
		else group.focusOn(view);
	}

	/** Joins the panes of tab `moving` into tab `target`, before its pane `at`, and shows
	 * `target` with the focus on the first pane that joined. False if that would be more
	 * than four panes. */
	joinTabs(target: TabGroup, moving: TabGroup, at = target.views.length): boolean {
		if (target === moving || target.views.length + moving.views.length > MAX_PANES) return false;
		const views = [...target.views];
		views.splice(Math.min(Math.max(0, at), views.length), 0, ...moving.views);
		this.groups = this.groups.filter((g) => g !== moving);
		const first = moving.views[0] ?? null;
		target.setViews(views, first);
		this.show(target, first);
		return true;
	}

	/** A second view of `view`'s document, beside it (after it, or before it), with the
	 * focus. False when its tab has four panes already. */
	showTwice(view: DocTab, before = false): boolean {
		const group = this.groupOf(view);
		if (!group || group.full) return false;
		const twin = DocTab.beside(view);
		const views = [...group.views];
		views.splice(group.views.indexOf(view) + (before ? 0 : 1), 0, twin);
		group.setViews(views, twin);
		this.show(group, twin);
		return true;
	}

	/** Takes a pane out of its joined tab (its header's close button): a second view of a
	 * document goes; a document shown once gets a tab of its own, right after. */
	takeOut(view: DocTab) {
		const group = this.groupOf(view);
		if (!group?.joined) return;
		group.setViews(group.views.filter((v) => v !== view));
		if (this.tabs.some((v) => v.id === view.id)) {
			view.dispose();
			return;
		}
		const groups = [...this.groups];
		groups.splice(groups.indexOf(group) + 1, 0, new TabGroup([view]));
		this.groups = groups;
	}

	/** Separates a joined tab into tabs of their own, in its place and order. Second views of
	 * a document go; the tab shown is the one with the pane that had the focus. */
	separate(group: TabGroup) {
		if (!group.joined || !this.groups.includes(group)) return;
		const focusedId = group.focused?.id ?? null;
		const kept: DocTab[] = [];
		for (const view of group.views) {
			if (kept.some((v) => v.id === view.id)) view.dispose();
			else kept.push(view);
		}
		const [first, ...others] = kept;
		if (!first) return;
		group.setViews([first]);
		const own = others.map((v) => new TabGroup([v]));
		const groups = [...this.groups];
		groups.splice(groups.indexOf(group) + 1, 0, ...own);
		this.groups = groups;
		if (this.activeKey === group.key && focusedId !== null && focusedId !== first.id) {
			const shown = own.find((g) => g.views[0]?.id === focusedId);
			if (shown) this.activeKey = shown.key;
		}
	}

	/** Closes the pane in focus of a joined tab (Close, Ctrl+W): a second view of a document
	 * just goes; a document shown once closes, after asking about unsaved changes. */
	async closePane(view: DocTab) {
		const group = this.groupOf(view);
		if (!group) return;
		if (this.tabs.some((v) => v !== view && v.id === view.id)) {
			const rest = group.views.filter((v) => v !== view);
			view.dispose();
			if (rest.length > 0) group.setViews(rest);
			else this.groups = this.groups.filter((g) => g !== group);
			return;
		}
		await this.closeTab(view.id);
	}

	/** Closes a tab: each of its documents, one after another, after asking about unsaved
	 * changes; it stops at the first the user keeps. */
	async closeGroup(group: TabGroup) {
		for (const id of [...new Set(group.views.map((v) => v.id))]) {
			await this.closeTab(id);
			if (this.tabs.some((t) => t.id === id)) return;
		}
	}

	/** Moves a tab into another window, all its documents: they arrive there as tabs of
	 * their own (a second view of a document stays behind and goes). */
	async moveGroupToWindow(group: TabGroup, target: MoveTarget) {
		let to = target;
		for (const id of [...new Set(group.views.map((v) => v.id))]) {
			const label = await this.moveToWindow(id, to);
			if (label === null) return;
			to = { kind: 'window', label };
		}
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
				opened: (result) => this.handleOpenResults([result]),
				smoothZoom: () => this.settings?.smoothZoom !== false
			});
		}
		this.startShown = false;
		this.combineActive = true;
	}

	private leaveCombine() {
		this.combineActive = false;
	}

	/** The grid of pages on screen, if one is: the Combine view's, or that of a document
	 * whose pages are being organized. Files dropped on the window go there. */
	get dropGrid(): 'combine' | 'organize' | null {
		if (this.combineActive) return 'combine';
		return !this.startShown && this.active?.organize ? 'organize' : null;
	}

	/** Tells Rust where dropped files go: opened as sources for a grid of pages, or as tabs. */
	syncDropTarget() {
		void setDropTarget(this.dropGrid !== null).catch(() => {});
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

	// ----- inserting pages from files (section 6.4) -----

	/** Whether pages can be inserted into `tab`; says why not. */
	canInsert(tab: DocTab): boolean {
		if (this.refuseReadOnly(tab)) return false;
		if (tab.flags.canAssemble) return true;
		void import('#lib/features/unprotect/actions.ts').then((m) =>
			m.refuseRestricted(tab, 'This document’s security settings don’t allow inserting pages.')
		);
		return false;
	}

	/** An insertion is under way: its files are being chosen or opened, or its options asked. */
	get inserting(): boolean {
		return this.pendingInsert !== null || this.insertRequest !== null;
	}

	/** Asks for files and then for the options; their pages go before page `at`, in the
	 * order of the files. `back` runs if the insertion is given up on the way. */
	async insertFromFile(tab: DocTab, at: number, back?: () => void) {
		if (this.inserting || !this.canInsert(tab)) {
			back?.();
			return;
		}
		this.pendingInsert = { tabId: tab.id, at: Math.min(at, tab.pageCount), back, slots: [], waiting: new Map() };
		try {
			this.insertSourcesOpened(await openInsertSources());
		} catch (e) {
			this.pendingInsert = null;
			back?.();
			this.showError(toAppError(e));
		}
	}

	/** Asks for the options of files opened already (dropped on the grid of pages), whose
	 * pages go before page `at`. */
	insertOpened(tab: DocTab, results: OpenResult[], at: number) {
		if (this.inserting || !this.canInsert(tab)) {
			this.discardSources(results);
			return;
		}
		this.pendingInsert = { tabId: tab.id, at: Math.min(at, tab.pageCount), slots: [], waiting: new Map() };
		this.insertSourcesOpened(results);
	}

	/** Closes files opened as sources that nothing will use. */
	discardSources(results: OpenResult[]) {
		for (const result of results) {
			if (result.kind === 'opened') void closeDocument(result.document.id).catch(() => {});
			else if (result.kind === 'needsPassword') void cancelUnlock(result.token).catch(() => {});
		}
	}

	/** The files of an insertion, opened or not: each has its place in their order, which
	 * a file that needs its password keeps until the password is given. */
	private insertSourcesOpened(results: OpenResult[]) {
		const pending = this.pendingInsert;
		if (!pending) {
			this.discardSources(results);
			return;
		}
		for (const result of results) this.insertSourceOpened(result, pending.slots.push(null) - 1);
		this.insertSourcesReady();
	}

	private insertSourceOpened(result: OpenResult, slot: number) {
		const pending = this.pendingInsert;
		if (!pending) {
			this.discardSources([result]);
			return;
		}
		if (result.kind === 'needsPassword') {
			pending.waiting.set(result.token, slot);
			this.askPassword(result.token, result.name, result.retry, 'insert');
		} else if (result.kind === 'failed') {
			this.notify(
				{ kind: 'error', message: `${result.name}: ${result.error.message}`, suggestion: result.error.suggestion },
				12000
			);
		} else if (result.kind === 'opened') {
			const source = result.document;
			if (source.flags.canCopy) {
				pending.slots[slot] = source;
				return;
			}
			void closeDocument(source.id).catch(() => {});
			this.notify(
				{
					kind: 'error',
					message: `${source.name} doesn’t allow copying its pages into another document.`,
					suggestion: 'Its security settings forbid it. Unlock it first (File > Unlock…), then add the unlocked file.'
				},
				12000
			);
		}
	}

	/** A password asked for a file of an insertion was given (`result`) or not (null). */
	private insertPasswordDone(token: number, result: OpenResult | null) {
		const pending = this.pendingInsert;
		const slot = pending?.waiting.get(token);
		if (!pending || slot === undefined) {
			if (result) this.discardSources([result]);
			return;
		}
		pending.waiting.delete(token);
		if (result) this.insertSourceOpened(result, slot);
		this.insertSourcesReady();
	}

	/** Once no password is waited for: the options are asked for the files that opened.
	 * If none did, the insertion ends. */
	private insertSourcesReady() {
		const pending = this.pendingInsert;
		if (!pending || pending.waiting.size > 0) return;
		this.pendingInsert = null;
		const sources = pending.slots.filter((source) => source !== null);
		if (sources.length > 0 && this.tabs.some((t) => t.id === pending.tabId)) {
			this.insertRequest = { tabId: pending.tabId, sources, at: pending.at, back: pending.back };
			return;
		}
		// (Or the document was closed meanwhile.)
		for (const source of sources) void closeDocument(source.id).catch(() => {});
		pending.back?.();
	}

	/**
	 * Inserts the files' pages before the place asked for, in the files' order: every page
	 * of each, or `pages` (0-based; all if empty) of a file that came alone. Each file is
	 * one step to undo.
	 */
	async insertPages(pages: number[], bookmarks: BookmarkMode, labels: InsertLabelMode): Promise<boolean> {
		const request = this.insertRequest;
		const tab = request && this.tabs.find((t) => t.id === request.tabId);
		if (!request || !tab) {
			this.cancelInsert();
			return false;
		}
		const signed = request.sources.filter((s) => s.flags.signed).map((s) => s.name);
		if (signed.length > 0 && (await this.ask(signedWarning(signed, true))) !== 'go') return false;
		const alone = request.sources.length === 1;
		const report: MergeReport = {
			pages: 0,
			renamedDestinations: 0,
			renamedFields: 0,
			renamedAttachments: 0,
			droppedLinks: 0,
			droppedBookmarks: 0,
			bookmarksSkipped: false
		};
		let files = 0;
		for (const source of request.sources) {
			const change = await this.apply(tab, {
				kind: 'insertPages',
				source: source.id,
				pages: alone ? pages : [],
				at: request.at + report.pages,
				bookmarks,
				labels
			});
			// (What went wrong has been said. The files before this one are in.)
			if (!change) break;
			files += 1;
			const part = change.mergeReport;
			report.pages += part?.pages ?? (alone && pages.length ? pages.length : source.pages.length);
			if (!part) continue;
			report.renamedDestinations += part.renamedDestinations;
			report.renamedFields += part.renamedFields;
			report.renamedAttachments += part.renamedAttachments;
			report.droppedLinks += part.droppedLinks;
			report.droppedBookmarks += part.droppedBookmarks;
			report.bookmarksSkipped ||= part.bookmarksSkipped;
		}
		if (files === 0) return false;
		this.insertRequest = null;
		for (const source of request.sources) void closeDocument(source.id).catch(() => {});
		this.activate(tab.id);
		// In the grid of pages, the inserted ones are selected, ready to be put elsewhere.
		if (tab.organize) tab.organize.showRange(request.at, report.pages);
		else tab.viewer?.goTo({ page: request.at, offset: 0 });
		const detail = reportDetail(report);
		const from = files === 1 ? request.sources[0]!.name : `${files} files`;
		this.notify(
			{
				kind: 'info',
				message: `Inserted ${report.pages === 1 ? '1 page' : `${report.pages.toLocaleString()} pages`} from ${from}.`,
				suggestion: detail
			},
			detail ? 15000 : 6000
		);
		return true;
	}

	/** Closes the insert dialog and the files it would have taken pages from. */
	cancelInsert() {
		const request = this.insertRequest;
		this.insertRequest = null;
		if (!request) return;
		for (const source of request.sources) void closeDocument(source.id).catch(() => {});
		request.back?.();
	}

	cycleTab(direction: 1 | -1) {
		const current = this.activeGroup;
		// From the start screen, back to the document it was shown over.
		if (this.startShown && current) {
			this.show(current);
			return;
		}
		if (this.groups.length < 2 || !current) return;
		const i = this.groups.indexOf(current);
		const next = this.groups[(i + direction + this.groups.length) % this.groups.length];
		if (next) this.show(next);
	}

	/** Moves the tab at `from` in the row to `to`. */
	moveTab(from: number, to: number) {
		const n = this.groups.length;
		if (from === to || from < 0 || to < 0 || from >= n || to >= n) return;
		const groups = [...this.groups];
		const [group] = groups.splice(from, 1);
		if (group) groups.splice(to, 0, group);
		this.groups = groups;
	}

	// ----- windows (section 8) -----

	/** The document's tab is all this window shows: in a new window it would be this window
	 * again. */
	isOnlyTab(id: number): boolean {
		return this.groups.length === 1 && !this.combine && this.groups[0]!.views.every((v) => v.id === id);
	}

	/** The tab is all this window shows. */
	isOnlyGroup(group: TabGroup): boolean {
		return this.groups.length === 1 && this.groups[0] === group && !this.combine;
	}

	/**
	 * Moves a tab into another window: a new one, one that is open, or wherever it was
	 * dropped. The document stays open in Rust as it is, unsaved changes and undo history
	 * included; the tab takes its place in the document and its page display along. A
	 * window left with nothing to show closes. Returns the window the tab went to, or null
	 * if it did not move.
	 */
	async moveToWindow(id: number, target: MoveTarget): Promise<string | null> {
		const tab = (this.active?.id === id ? this.active : null) ?? this.tabs.find((t) => t.id === id);
		// Not in the middle of a save, whose result this window's tab is waiting for.
		if (!tab || tab.saving || this.moving.has(id)) return null;
		this.moving.add(id);
		try {
			if (this.insertRequest?.tabId === id) this.cancelInsert();
			await this.settled();
			// Where the user is: the tab opens there in its new window.
			await rememberView(tab.id, tab.viewState()).catch(() => {});
			const label = await moveDocument(id, target, tab.handoff());
			if (label === null) return null;
			this.removeDocument(id);
			if (this.tabs.length === 0 && !this.combine) void getCurrentWindow().close();
			return label;
		} catch (e) {
			this.showError(toAppError(e));
			return null;
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

	/** Takes a document out of this window, every view of it: a joined tab keeps its other
	 * panes; a tab left empty goes, and the one after it shows, or what else is open. */
	private removeDocument(id: number) {
		const before = this.activeGroup;
		const index = before ? this.groups.indexOf(before) : -1;
		const groups: TabGroup[] = [];
		for (const group of this.groups) {
			const rest = group.views.filter((v) => v.id !== id);
			for (const view of group.views) if (view.id === id) view.dispose();
			if (rest.length === 0) continue;
			if (rest.length !== group.views.length) group.setViews(rest);
			groups.push(group);
		}
		this.groups = groups;
		if (before && !groups.includes(before)) {
			const next = groups[Math.min(index, groups.length - 1)];
			this.activeKey = next?.key ?? null;
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
			title: tab.untitled ? 'Save this document?' : 'Save changes?',
			message: tab.untitled
				? `${tab.name} has not been saved yet. Do you want to save it?`
				: `Do you want to save the changes to ${tab.name}?`,
			detail: tab.untitled ? 'If you don’t save, the document will be lost.' : 'If you don’t save, your changes will be lost.',
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
		if (this.labelsRequest?.tabId === id) this.labelsRequest = null;
		await this.settled();
		if (tab.state.dirty) this.activate(id);
		if (!(await this.confirmDiscard(tab))) return;
		// Moved to another window while it asked: it is that window's to close now.
		if (!this.tabs.includes(tab)) return;
		this.rememberView(tab);
		this.removeDocument(id);
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
		// Tabs Rust has just handed this window are in before it answers: asked to close a
		// moment after it was made for a tab (File > Exit in another window), it would
		// otherwise see no unsaved changes and close with them.
		await this.adopting;
		const dirty = this.documents.filter((t) => t.state.dirty);
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
	/** Says that a save took the PDF/A claim out of a file: because of what was changed, or
	 * because it was given a password or restrictions, which PDF/A forbids (section 6.9). */
	notifyPdfaRemoved(name: string, level: PdfaLevel, reason: 'edits' | 'protection' | 'compression' = 'edits') {
		this.notify(
			{
				kind: 'info',
				message: `${name} no longer claims to be ${pdfaName(level)}.`,
				suggestion:
					reason === 'protection'
						? 'PDF/A doesn’t allow a password or restrictions, so Lectrix removed the claim when saving the file with them.'
						: reason === 'compression'
							? 'Compressing rewrites a file’s images, fonts and structure, and Lectrix can’t vouch that the result still complies with that standard, so it removed the claim.'
							: 'Annotations, and pages inserted or deleted, may leave a file that no longer complies with that standard, so Lectrix removed the claim when saving. Changes to bookmarks, page labels, page rotation and the order of the pages alone keep it.'
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
		const repaired = tab.fileFlags.repaired;
		const protecting = tab.state.protection?.kind === 'set';
		tab.state = result.state;
		tab.applySaved(result);
		tab.name = result.name;
		tab.path = result.path;
		// Saved, a document is its file's from then on.
		tab.untitled = false;
		tab.banner = null;
		if (result.pdfaRemoved) this.notifyPdfaRemoved(tab.name, result.pdfaRemoved, protecting ? 'protection' : 'edits');
		if (result.fellBackToFull && repaired) {
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

	/** Saves in place; a document not saved under a name yet asks where to save, as Save As
	 * does. Returns null if the save failed or was cancelled. */
	async save(tab: DocTab): Promise<SaveResult | null> {
		if (tab.untitled) return this.saveAs(tab);
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
		// A document not saved under a name yet has no file to read again.
		if (tab.untitled) return;
		try {
			this.showReloaded(tab, await reloadDocument(tab.id));
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	/** Shows `tab`'s document as read again from its file (`info`), each view where it was. */
	showReloaded(tab: DocTab, info: DocumentInfo) {
		const views = [...tab.doc.views];
		const positions = views.map((v) => v.viewer?.position());
		tab.update(info);
		tab.banner = null;
		views.forEach((v, i) => {
			const position = positions[i];
			if (position) v.viewer?.goTo(position, { recordHistory: false });
			v.search.refresh();
		});
	}
}

export const app = new AppStore();
