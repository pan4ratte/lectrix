import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { UpdateInfo, UpdateProgress, UpdateReady } from '#lib/ipc/index.ts';

const ipc = vi.hoisted(() => ({
	checkForUpdate: vi.fn<(manual?: boolean) => Promise<UpdateInfo | null>>(),
	skipUpdate: vi.fn<(version: string) => Promise<void>>(async () => {}),
	downloadUpdate: vi.fn<(onProgress: (p: UpdateProgress) => void) => Promise<UpdateReady>>(),
	cancelUpdateDownload: vi.fn<() => Promise<void>>(async () => {}),
	restartToUpdate: vi.fn<() => Promise<void>>(async () => {})
}));
const store = vi.hoisted(() => ({
	showError: vi.fn(),
	notify: vi.fn(),
	quitWith: vi.fn(async (quit: () => Promise<void>) => {
		await quit();
		return true;
	})
}));

vi.mock('#lib/ipc/index.ts', () => ({
	...ipc,
	toAppError: (e: unknown) => e
}));
vi.mock('#lib/stores/app.svelte.ts', () => ({ app: store }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '1.1.0' }));

const { downloadShare, downloadStatus, update } = await import('./update.svelte.ts');

const info: UpdateInfo = { version: '1.2.0', currentVersion: '1.1.0' };

beforeEach(() => {
	vi.clearAllMocks();
	update.stage = { kind: 'idle' };
	update.downloaded = null;
});

describe('download progress', () => {
	it('shows megabytes, with the total when it is known', () => {
		expect(downloadStatus(null)).toBe('Starting the download…');
		expect(downloadStatus({ downloaded: 3 * 1024 * 1024, total: 12 * 1024 * 1024 })).toBe('3.0 of 12.0 MB');
		expect(downloadStatus({ downloaded: 1572864, total: null })).toBe('1.5 MB downloaded');
		expect(downloadShare({ downloaded: 5, total: 10 })).toBe(0.5);
		expect(downloadShare({ downloaded: 5, total: null })).toBeNull();
	});
});

describe('update notice', () => {
	it('appears only when a newer release is found', async () => {
		ipc.checkForUpdate.mockResolvedValueOnce(null);
		await update.check();
		expect(update.stage.kind).toBe('idle');
		ipc.checkForUpdate.mockResolvedValueOnce(info);
		await update.check();
		expect(update.stage).toEqual({ kind: 'available', info });
	});

	it('Not now hides it without remembering anything', async () => {
		update.stage = { kind: 'available', info };
		update.notNow();
		expect(update.stage.kind).toBe('idle');
		expect(ipc.skipUpdate).not.toHaveBeenCalled();
	});

	it('Don’t ask again skips this version', async () => {
		update.stage = { kind: 'available', info };
		await update.dontAskAgain();
		expect(update.stage.kind).toBe('idle');
		expect(ipc.skipUpdate).toHaveBeenCalledWith('1.2.0');
	});

	it('downloads with progress, then offers a restart', async () => {
		update.stage = { kind: 'available', info };
		let seen: UpdateProgress | null = null;
		ipc.downloadUpdate.mockImplementationOnce(async (onProgress) => {
			onProgress({ downloaded: 50, total: 100 });
			if (update.stage.kind === 'downloading') seen = update.stage.progress;
			return { installed: false };
		});
		await update.update();
		expect(seen).toEqual({ downloaded: 50, total: 100 });
		expect(update.stage).toEqual({ kind: 'ready', info, installed: false, restarting: false });
		await update.restart();
		expect(store.quitWith).toHaveBeenCalledOnce();
		expect(ipc.restartToUpdate).toHaveBeenCalledOnce();
	});

	it('offers the choices again when the download is stopped, and reports failures', async () => {
		update.stage = { kind: 'available', info };
		ipc.downloadUpdate.mockRejectedValueOnce({ message: 'stopped', suggestion: null, code: 'cancelled' });
		await update.update();
		expect(update.stage).toEqual({ kind: 'available', info });
		expect(store.showError).not.toHaveBeenCalled();

		ipc.downloadUpdate.mockRejectedValueOnce({ message: 'offline', suggestion: null, code: 'general' });
		await update.update();
		expect(update.stage.kind).toBe('idle');
		expect(store.showError).toHaveBeenCalledOnce();
	});

	it('stays ready when the user cancels the restart at a save prompt', async () => {
		update.stage = { kind: 'ready', info, installed: true, restarting: false };
		store.quitWith.mockResolvedValueOnce(false);
		await update.restart();
		expect(update.stage).toEqual({ kind: 'ready', info, installed: true, restarting: false });
	});
});

describe('Help > Check for updates', () => {
	it('asks whatever Settings say, and offers the update it finds', async () => {
		let during = '';
		ipc.checkForUpdate.mockImplementationOnce(async () => {
			during = update.stage.kind;
			return info;
		});
		await update.checkNow();
		expect(ipc.checkForUpdate).toHaveBeenCalledWith(true);
		expect(during).toBe('checking');
		expect(update.stage).toEqual({ kind: 'available', info });
	});

	it('says when this version is the latest', async () => {
		ipc.checkForUpdate.mockResolvedValueOnce(null);
		await update.checkNow();
		expect(update.stage.kind).toBe('idle');
		expect(store.notify).toHaveBeenCalledWith({
			kind: 'info',
			message: 'Lectrix is up to date',
			suggestion: 'You have the latest version, 1.1.0.'
		});
	});

	it('says why it could not ask', async () => {
		const error = { message: 'Lectrix couldn’t check for updates.', suggestion: 'Check your internet connection.', code: 'general' };
		ipc.checkForUpdate.mockRejectedValueOnce(error);
		await update.checkNow();
		expect(update.stage.kind).toBe('idle');
		expect(store.showError).toHaveBeenCalledWith(error);
	});

	it('leaves a notice on screen as it is', async () => {
		update.stage = { kind: 'available', info };
		await update.checkNow();
		expect(ipc.checkForUpdate).not.toHaveBeenCalled();
		expect(update.stage).toEqual({ kind: 'available', info });
	});

	it('offers the restart again for an update downloaded and put off', async () => {
		update.stage = { kind: 'available', info };
		ipc.downloadUpdate.mockResolvedValueOnce({ installed: false });
		await update.update();
		update.later();
		await update.checkNow();
		expect(ipc.checkForUpdate).not.toHaveBeenCalled();
		expect(update.stage).toEqual({ kind: 'ready', info, installed: false, restarting: false });
	});
});
