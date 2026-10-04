// App-wide state: tabs, dialogs, notifications, and the flows that span them (open, save,
// close, exit). Rust owns the documents; this owns what the window shows.

import {
	applyOperation,
	cancelUnlock,
	closeDocument,
	discardRecovered,
	exitConfirmed,
	listOpenDocuments,
	listRecentFiles,
	listRecovered,
	openInsertSource,
	openRecent,
	openStartupDocuments,
	openWithDialog,
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
	type OpenResult,
	type OperationInput,
	type PaneLayout,
	type RecentFile,
	type SaveResult,
	type Settings,
	type StartupInfo
} from '#lib/ipc/index.ts';
import { ANNOTATIONS_LIMITS, SIDEBAR_LIMITS, clampWidth } from '#lib/components/panes.ts';
import { CombineState } from '#lib/features/merge/combine.svelte.ts';
import { reportDetail, signedWarning } from '#lib/features/merge/pages.ts';
import { discardQuestion, recoveryQuestion } from '#lib/features/recovery/recovery.ts';

import { DocTab } from './doc.svelte.ts';

export interface DialogButton {
	id: string;
	label: string;
	primary?: boolean;
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

class AppStore {
	tabs = $state<DocTab[]>([]);
	activeId = $state<number | null>(null);
	startup = $state<StartupInfo | null>(null);
	recent = $state<RecentFile[]>([]);
	/** The left sidebar (section 8): open, its width, and the panel it shows. */
	sidebarOpen = $state(true);
	sidebarWidth = $state(SIDEBAR_LIMITS.initial);
	sidebarPanel = $state<'pages' | 'bookmarks' | 'labels'>('pages');
	/** The right pane with the annotation list. */
	annotationsOpen = $state(false);
	annotationsWidth = $state(ANNOTATIONS_LIMITS.initial);
	/** The remembered pane layout is in; until then panes neither animate nor are saved. */
	panesRestored = $state(false);
	/** The inspector (properties of the selected bookmark) is open. */
	inspectorOpen = $state(false);
	/** The annotation inspector is open; it shows the selected annotation until closed or
	 * until nothing is selected. A click on an annotation shows only its bar. */
	annotationInspectorOpen = $state(false);
	/** A note was just placed: the annotation inspector focuses its text. */
	focusNoteText = $state(false);
	/** The Settings dialog is open. */
	settingsOpen = $state(false);
	/** The stored settings, once loaded. */
	settings = $state<Settings | null>(null);
	/** Where the search bar and inspectors start, CSS pixels from the top of the page
	 * canvas: below the annotation toolbar when it stays there. */
	overlayTop = $derived(
		this.settings?.toolbarPosition === 'top' && this.settings.toolbarVisibility === 'always' ? 64 : 12
	);
	/** The About dialog is open. */
	aboutOpen = $state(false);
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
	private exiting = false;

	/** The document on screen; null while the Combine view is. */
	active = $derived(this.combineActive ? null : (this.tabs.find((t) => t.id === this.activeId) ?? null));

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
		this.sidebarOpen = panes.sidebarOpen;
		this.sidebarWidth = clampWidth(panes.sidebarWidth, SIDEBAR_LIMITS);
		this.annotationsOpen = panes.annotationsOpen;
		this.annotationsWidth = clampWidth(panes.annotationsWidth, ANNOTATIONS_LIMITS);
		this.panesRestored = true;
	}

	get panes(): PaneLayout {
		return {
			sidebarOpen: this.sidebarOpen,
			sidebarWidth: this.sidebarWidth,
			annotationsOpen: this.annotationsOpen,
			annotationsWidth: this.annotationsWidth
		};
	}

	// ----- opening -----

	async refreshRecent() {
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
						this.tabs = [...this.tabs, new DocTab(result.document)];
					}
					activate = result.document.id;
					break;
				}
				case 'alreadyOpen':
					activate = result.id;
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
			// Documents Rust already has open come back first: the page may have reloaded
			// (a renderer crash, or a test driver navigating it) after they were opened.
			const open = await listOpenDocuments();
			if (open.length) this.handleOpenResults(open.map((document) => ({ kind: 'opened', document })));
			this.handleOpenResults(await openStartupDocuments());
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
					return tab ? (await this.save(tab)) !== null : true;
				},
				opened: (result) => this.handleOpenResults([result])
			});
		}
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
		await combine.dispose();
	}

	// ----- inserting pages from a file (section 6.4) -----

	/** Asks for a file and then for the options; pages go before page `at` by default. */
	async insertFromFile(tab: DocTab, at = tab.currentPage + 1) {
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
		this.activate(id);
		if (this.insertRequest?.tabId === id) this.cancelInsert();
		await this.settled();
		if (!(await this.confirmDiscard(tab))) return;
		this.rememberView(tab);
		tab.search.cancel();
		const index = this.tabs.indexOf(tab);
		this.tabs = this.tabs.filter((t) => t !== tab);
		if (this.activeId === id) {
			const next = this.tabs[Math.min(index, this.tabs.length - 1)];
			this.activeId = next?.id ?? null;
			// The last document closed while the Combine view is open: show it.
			if (!next && this.combine) this.openCombine();
		}
		await closeDocument(id).catch(() => {});
		void this.refreshRecent();
	}

	/**
	 * Handles the window's close request: prompts about unsaved documents. Returns true if
	 * the window may close.
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
		if (result.fellBackToFull && tab.flags.repaired) {
			tab.flags = { ...tab.flags, repaired: false };
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
