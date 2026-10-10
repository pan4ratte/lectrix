// Updates from GitHub releases (ADR 0011). After startup, then every hour while it runs,
// Lectrix asks Rust whether a newer release exists; Rust asks GitHub once a day. A notice
// offers Update, Not now (asked again at the next day's check) and Don't ask again (this
// version is never offered again). Updating downloads with progress, then offers
// a restart: on Windows the installer runs as Lectrix restarts, or when it closes.
// Help > Check for updates asks at any time, and says what it found either way.
// The Debug menu rehearses the notice (`simulate`): its buttons then work on a stand-in for
// Rust, which downloads nothing and restarts nothing.
import { getVersion } from '@tauri-apps/api/app';

import { APP_NAME } from '#lib/config.ts';
import {
	cancelUpdateDownload,
	checkForUpdate,
	downloadUpdate,
	restartToUpdate,
	skipUpdate,
	toAppError,
	type AppError,
	type UpdateInfo,
	type UpdateProgress,
	type UpdateReady
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

/** What the notice's buttons ask of Rust. */
interface Backend {
	download(onProgress: (progress: UpdateProgress) => void): Promise<UpdateReady>;
	cancel(): Promise<void>;
	restart(): Promise<void>;
	skip(version: string): Promise<void>;
}

const RUST: Backend = {
	download: downloadUpdate,
	cancel: cancelUpdateDownload,
	restart: async () => {
		await app.quitWith(restartToUpdate);
	},
	skip: skipUpdate
};

/** How long a rehearsed download takes, and its pretend size. */
const REHEARSAL_MS = 3000;
const REHEARSAL_BYTES = 14.2 * MB;

/** A stand-in for Rust while the Debug menu rehearses the notice: the download counts up
 * to its size and can be stopped; restarting says what would happen. `installed` as on
 * macOS and Linux, else as on Windows. */
function rehearsal(installed: boolean): Backend {
	let stop: (() => void) | null = null;
	return {
		download: (onProgress) =>
			new Promise((done, fail) => {
				const started = Date.now();
				const timer = setInterval(() => {
					const share = Math.min(1, (Date.now() - started) / REHEARSAL_MS);
					onProgress({ downloaded: Math.round(share * REHEARSAL_BYTES), total: REHEARSAL_BYTES });
					if (share === 1) {
						clearInterval(timer);
						done({ installed });
					}
				}, 100);
				stop = () => {
					clearInterval(timer);
					const stopped: AppError = { message: 'The download was stopped.', suggestion: null, code: 'cancelled' };
					fail(stopped);
				};
			}),
		cancel: async () => stop?.(),
		restart: async () => {
			app.notify({ kind: 'info', message: 'Rehearsal: Lectrix would restart now.', suggestion: 'Nothing was installed.' });
			update.stage = { kind: 'idle' };
		},
		skip: async () => {}
	};
}

/** Help > Check for updates found nothing newer. */
export function notifyUpToDate(version: string | null) {
	app.notify({
		kind: 'info',
		message: `${APP_NAME} is up to date`,
		suggestion: version ? `You have the latest version, ${version}.` : 'You have the latest version.'
	});
}

class UpdateState {
	stage = $state<UpdateStage>({ kind: 'idle' });
	#backend = RUST;
	/** Counts rehearsals: a download from before the latest one leaves the notice alone
	 * when it ends, since the notice now shows something else. */
	#rehearsals = 0;
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
		this.#backend = RUST;
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
		this.#backend = RUST;
		this.stage = { kind: 'checking' };
		try {
			const info = await checkForUpdate(true);
			if (this.stage.kind !== 'checking') return;
			if (info) {
				this.stage = { kind: 'available', info };
				return;
			}
			this.stage = { kind: 'idle' };
			notifyUpToDate(await getVersion().catch(() => null));
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
		await this.#backend.skip(version).catch((e: unknown) => app.showError(toAppError(e)));
	}

	async update() {
		if (this.stage.kind !== 'available') return;
		const info = this.stage.info;
		const backend = this.#backend;
		const rehearsal = this.#rehearsals;
		const current = () => rehearsal === this.#rehearsals;
		this.stage = { kind: 'downloading', info, progress: null, stopping: false };
		try {
			const ready = await backend.download((progress) => {
				if (current() && this.stage.kind === 'downloading') this.stage.progress = progress;
			});
			if (!current()) return;
			if (backend === RUST) this.downloaded = { info, installed: ready.installed };
			this.stage = { kind: 'ready', info, installed: ready.installed, restarting: false };
		} catch (e) {
			if (!current()) return;
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
		await this.#backend.cancel().catch(() => {});
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
			await this.#backend.restart();
		} catch (e) {
			app.showError(toAppError(e));
		}
		// Still running: the user cancelled at a prompt, or the restart failed.
		if (this.stage.kind === 'ready') this.stage.restarting = false;
	}

	/** Shows the notice at `stage` for the Debug menu; its buttons then rehearse instead of
	 * asking Rust (`installed`: as on macOS and Linux). A download in progress counts up. */
	simulate(stage: UpdateStage, installed = false) {
		this.endRehearsal();
		this.#backend = rehearsal(installed);
		this.stage = stage;
		if (stage.kind === 'downloading') {
			this.stage = { kind: 'available', info: stage.info };
			void this.update();
		}
	}

	/** Takes a rehearsed notice away, stopping its download; a real notice stays. */
	endRehearsal() {
		if (this.#backend === RUST) return;
		this.#rehearsals++;
		void this.#backend.cancel().catch(() => {});
		this.#backend = RUST;
		this.stage = { kind: 'idle' };
	}
}

export const update = new UpdateState();
