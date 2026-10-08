// Updates from GitHub releases (ADR 0011). After startup, then every hour while it runs,
// Lectrix asks Rust whether a newer release exists; Rust asks GitHub once a day. A notice
// offers Update, Not now (asked again at the next day's check) and Don't ask again (this
// version is never offered again). Updating downloads with progress, then offers
// a restart: on Windows the installer runs as Lectrix restarts, or when it closes.
// Help > Check for updates asks at any time, and says what it found either way.
import { getVersion } from '@tauri-apps/api/app';

import { APP_NAME } from '#lib/config.ts';
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
	/** Help > Check for updates, waiting for GitHub. */
	| { kind: 'checking' }
	| { kind: 'available'; info: UpdateInfo }
	| { kind: 'downloading'; info: UpdateInfo; progress: UpdateProgress | null; stopping: boolean }
	/** `installed`: macOS and Linux install at once; Windows installs on restart or exit. */
	| { kind: 'ready'; info: UpdateInfo; installed: boolean; restarting: boolean };

/** How often the automatic check asks Rust, which asks GitHub only once a day: hourly, so a
 * Lectrix left open for days still checks daily, and soon after the computer wakes. */
export const CHECK_EVERY_MS = 60 * 60 * 1000;

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
	/** The update downloaded this run, kept after Later so asking again offers the restart
	 * instead of downloading it again. */
	downloaded: { info: UpdateInfo; installed: boolean } | null = null;

	/** Checks now and then every hour from now on (ADR 0011). */
	startChecks() {
		void this.check();
		setInterval(() => void this.check(), CHECK_EVERY_MS);
	}

	/** Looks for an update in the background; shows the notice if there is one. Not while a
	 * notice is up, or once an update is downloaded: Restart now or Later has been offered. */
	async check() {
		if (this.stage.kind !== 'idle' || this.downloaded) return;
		try {
			const info = await checkForUpdate();
			if (info && this.stage.kind === 'idle') this.stage = { kind: 'available', info };
		} catch {
			// Rust logs failures and reports them as no update; nothing to show.
		}
	}

	/** Help > Check for updates: whatever Settings say, a skipped version included. Shows
	 * what it finds: the update, that this version is the latest, or why it couldn't ask. */
	async checkNow() {
		// The notice is up already (an update found, downloading, checking): it stays.
		if (this.stage.kind !== 'idle') return;
		if (this.downloaded) {
			this.stage = { kind: 'ready', ...this.downloaded, restarting: false };
			return;
		}
		this.stage = { kind: 'checking' };
		try {
			const info = await checkForUpdate(true);
			if (this.stage.kind !== 'checking') return;
			if (info) {
				this.stage = { kind: 'available', info };
				return;
			}
			this.stage = { kind: 'idle' };
			const version = await getVersion().catch(() => null);
			app.notify({
				kind: 'info',
				message: `${APP_NAME} is up to date`,
				suggestion: version ? `You have the latest version, ${version}.` : 'You have the latest version.'
			});
		} catch (e) {
			if (this.stage.kind === 'checking') this.stage = { kind: 'idle' };
			app.showError(toAppError(e));
		}
	}

	/** Hides the notice; the next day's check asks again. */
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
			this.downloaded = { info, installed: ready.installed };
			this.stage = { kind: 'ready', info, installed: ready.installed, restarting: false };
		} catch (e) {
			const error = toAppError(e);
			// Stopped: offer the choices again. Failed: say why; the next day's check asks again.
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
