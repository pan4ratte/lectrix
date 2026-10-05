// Updates from GitHub releases (ADR 0011). After startup Lectrix asks Rust whether a newer
// release exists; a notice offers Update, Not now (asked again next launch) and Don't ask
// again (this version is never offered again). Updating downloads with progress, then offers
// a restart: on Windows the installer runs as Lectrix restarts, or when it closes.
import {
	cancelUpdateDownload,
	checkForUpdate,
	downloadUpdate,
	restartToUpdate,
	skipUpdate,
	toAppError,
	type UpdateInfo,
	type UpdateProgress
} from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';

export type UpdateStage =
	| { kind: 'idle' }
	| { kind: 'available'; info: UpdateInfo }
	| { kind: 'downloading'; info: UpdateInfo; progress: UpdateProgress | null; stopping: boolean }
	/** `installed`: macOS and Linux install at once; Windows installs on restart or exit. */
	| { kind: 'ready'; info: UpdateInfo; installed: boolean; restarting: boolean };

const MB = 1024 * 1024;

function megabytes(bytes: number): string {
	return (bytes / MB).toFixed(1);
}

/** How much is downloaded, as a fraction, or null when the size is unknown. */
export function downloadShare(progress: UpdateProgress | null): number | null {
	if (!progress?.total) return null;
	return Math.min(1, progress.downloaded / progress.total);
}

export function downloadStatus(progress: UpdateProgress | null): string {
	if (!progress) return 'Starting the download…';
	if (!progress.total) return `${megabytes(progress.downloaded)} MB downloaded`;
	return `${megabytes(progress.downloaded)} of ${megabytes(progress.total)} MB`;
}

class UpdateState {
	stage = $state<UpdateStage>({ kind: 'idle' });

	/** Looks for an update in the background; shows the notice if there is one. */
	async check() {
		try {
			const info = await checkForUpdate();
			if (info && this.stage.kind === 'idle') this.stage = { kind: 'available', info };
		} catch {
			// Rust logs failures and reports them as no update; nothing to show.
		}
	}

	/** Hides the notice; the next launch asks again. */
	notNow() {
		this.stage = { kind: 'idle' };
	}

	async dontAskAgain() {
		if (this.stage.kind !== 'available') return;
		const { version } = this.stage.info;
		this.stage = { kind: 'idle' };
		await skipUpdate(version).catch((e: unknown) => app.showError(toAppError(e)));
	}

	async update() {
		if (this.stage.kind !== 'available') return;
		const info = this.stage.info;
		this.stage = { kind: 'downloading', info, progress: null, stopping: false };
		try {
			const ready = await downloadUpdate((progress) => {
				if (this.stage.kind === 'downloading') this.stage.progress = progress;
			});
			this.stage = { kind: 'ready', info, installed: ready.installed, restarting: false };
		} catch (e) {
			const error = toAppError(e);
			// Stopped: offer the choices again. Failed: say why; the next launch asks again.
			if (error.code === 'cancelled') {
				this.stage = { kind: 'available', info };
			} else {
				this.stage = { kind: 'idle' };
				app.showError(error);
			}
		}
	}

	async stop() {
		if (this.stage.kind !== 'downloading') return;
		this.stage.stopping = true;
		await cancelUpdateDownload().catch(() => {});
	}

	/** Hides the notice. Windows installs the update when Lectrix closes. */
	later() {
		this.stage = { kind: 'idle' };
	}

	/** Asks about unsaved documents as closing does, then restarts into the new version. */
	async restart() {
		if (this.stage.kind !== 'ready') return;
		this.stage.restarting = true;
		try {
			await app.quitWith(restartToUpdate);
		} catch (e) {
			app.showError(toAppError(e));
		}
		// Still running: the user cancelled at a prompt, or the restart failed.
		if (this.stage.kind === 'ready') this.stage.restarting = false;
	}
}

export const update = new UpdateState();
