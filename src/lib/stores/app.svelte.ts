// App-wide state: tabs, dialogs, notifications, and the flows that span them (open, save,
// close, exit). Rust owns the documents; this owns what the window shows.

import {
	applyOperation,
	cancelUnlock,
	closeDocument,
	listRecentFiles,
	openRecent,
	openStartupDocuments,
	openWithDialog,
	redo as redoCommand,
	reloadDocument,
	rememberView,
	removeRecent,
	save as saveCommand,
	saveAs as saveAsCommand,
	toAppError,
	undo as undoCommand,
	unlockDocument,
	type AppError,
	type DocumentChange,
	type OpenResult,
	type OperationInput,
	type RecentFile,
	type SaveResult,
	type StartupInfo
} from '#lib/ipc/index.ts';

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
	sidebarOpen = $state(true);
	/** The panel shown in the sidebar. */
	sidebarPanel = $state<'pages' | 'bookmarks'>('pages');
	/** The inspector (properties of the selected bookmark) is open. */
	inspectorOpen = $state(false);
	dialog = $state<DialogRequest | null>(null);
	passwordPrompts = $state<PasswordRequest[]>([]);
	toasts = $state<Toast[]>([]);
	/** Shown while files are dragged over the window. */
	dropTarget = $state(false);
	rotateDialogOpen = $state(false);
	private exiting = false;

	active = $derived(this.tabs.find((t) => t.id === this.activeId) ?? null);

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
					const tab = new DocTab(result.document);
					this.tabs = [...this.tabs, tab];
					activate = tab.id;
					break;
				}
				case 'alreadyOpen':
					activate = result.id;
					break;
				case 'needsPassword':
					this.passwordPrompts = [
						...this.passwordPrompts,
						{ token: result.token, name: result.name, retry: result.retry }
					];
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
			this.handleOpenResults(await openStartupDocuments());
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	async submitPassword(token: number, password: string | null) {
		this.passwordPrompts = this.passwordPrompts.filter((p) => p.token !== token);
		if (password === null) {
			await cancelUnlock(token);
			return;
		}
		try {
			this.handleOpenResults([await unlockDocument(token, password)]);
		} catch (e) {
			this.showError(toAppError(e));
		}
	}

	// ----- tabs -----

	activate(id: number) {
		if (this.activeId === id) return;
		const previous = this.active;
		if (previous) this.rememberView(previous);
		this.activeId = id;
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
		if (!(await this.confirmDiscard(tab))) return;
		this.rememberView(tab);
		tab.search.cancel();
		const index = this.tabs.indexOf(tab);
		this.tabs = this.tabs.filter((t) => t !== tab);
		if (this.activeId === id) {
			const next = this.tabs[Math.min(index, this.tabs.length - 1)];
			this.activeId = next?.id ?? null;
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
		this.exiting = true;
		return true;
	}

	// ----- editing -----

	/** Warns once before the first edit of a signed document (section 5.4). */
	private async allowEdit(tab: DocTab): Promise<boolean> {
		if (!tab.flags.signed || tab.signedWarningAccepted) return true;
		const choice = await this.ask({
			title: 'This document is signed',
			message: 'Changes you make will show as edits made after it was signed.',
			detail:
				'The signature stays valid for the signed version, and Folio saves your changes after it without altering the signed part.',
			buttons: [
				{ id: 'edit', label: 'Edit anyway', primary: true },
				{ id: 'cancel', label: 'Cancel' }
			],
			cancel: 'cancel'
		});
		tab.signedWarningAccepted = choice === 'edit';
		return tab.signedWarningAccepted;
	}

	/** Applies an operation. Returns what changed, or null if it was cancelled or failed. */
	async apply(tab: DocTab, operation: OperationInput): Promise<DocumentChange | null> {
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
		tab.name = result.name;
		tab.path = result.path;
		tab.banner = null;
		if (result.fellBackToFull && tab.flags.repaired) {
			tab.flags = { ...tab.flags, repaired: false };
			this.notify(
				{
					kind: 'info',
					message: `${tab.name} was damaged, so Folio saved a complete, repaired copy instead of adding your changes to the end of the file.`,
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
