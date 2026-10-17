// The Debug menu, after Help, in development builds or with LECTRIX_DEBUG=1: shows each
// notification, notice and prompt on demand (rehearse.ts), so they can be looked at and
// clicked through without an update, a crash or another program.

import { toAppError } from '#lib/ipc/index.ts';
import type { AppMenu, MenuEntry } from '#lib/menus.svelte.ts';
import { app } from '#lib/stores/app.svelte.ts';

import {
	clearRehearsals,
	rehearseBanner,
	rehearseError,
	rehearseFullScreen,
	rehearseInfo,
	rehearsePagesDeleted,
	rehearsePdfa,
	rehearsePdfaRemoved,
	rehearseRecovery,
	rehearseUpdate,
	rehearseUpdated,
	rehearseUpdateError,
	rehearseUpToDate
} from './rehearse.ts';

/** Runs a rehearsal, saying so when it fails rather than showing nothing. */
function run(rehearsal: () => Promise<unknown>) {
	return () => void rehearsal().catch((e: unknown) => app.showError(toAppError(e)));
}

function item(id: string, label: string, onSelect: () => void, enabled = true): MenuEntry {
	return { kind: 'item', id: `debug:${id}`, label, run: onSelect, enabled };
}

function submenu(label: string, items: MenuEntry[], enabled = true): MenuEntry {
	return { kind: 'submenu', label, items, enabled };
}

const separator: MenuEntry = { kind: 'separator' };

export function debugMenu(): AppMenu {
	const hasDoc = app.active !== null;
	/** The start screen is on screen: no document, and not the Combine view. */
	const onStartScreen = app.active === null && !app.combineActive;
	return {
		label: 'Debug',
		items: [
			submenu('Update notice', [
				item('update-checking', 'Checking for updates', run(() => rehearseUpdate('checking'))),
				item('update-available', 'Update available', run(() => rehearseUpdate('available'))),
				item('update-downloading', 'Downloading', run(() => rehearseUpdate('downloading'))),
				item('update-ready', 'Ready to install (Windows)', run(() => rehearseUpdate('ready'))),
				item('update-installed', 'Installed (macOS, Linux)', run(() => rehearseUpdate('installed'))),
				separator,
				item('up-to-date', 'Up to date', run(() => rehearseUpToDate())),
				item('check-failed', 'Check failed', () => rehearseUpdateError('check')),
				item('download-failed', 'Download failed', () => rehearseUpdateError('download'))
			]),
			submenu('After an update', [
				item('updated-start', 'On the start screen', run(() => rehearseUpdated('start screen')), onStartScreen),
				item('updated-notification', 'As a notification', run(() => rehearseUpdated('notification')))
			]),
			submenu('Notification', [
				item('info', 'Information', rehearseInfo),
				item('error', 'Error', () => rehearseError(false)),
				item('error-button', 'Error with a button', () => rehearseError(true)),
				item('pdfa-removed', 'PDF/A claim removed', rehearsePdfaRemoved),
				item('full-screen', 'Full screen started', rehearseFullScreen),
				item('pages-deleted', 'Pages deleted', rehearsePagesDeleted)
			]),
			submenu(
				'File changed on disk',
				[
					item('changed', 'Changed', () => rehearseBanner('changedOnDisk')),
					item('changed-dirty', 'Changed, with unsaved changes', () => rehearseBanner('changedOnDiskDirty')),
					item('removed', 'Removed', () => rehearseBanner('deletedOnDisk'))
				],
				hasDoc
			),
			item('pdfa', 'PDF/A file, for reading only', rehearsePdfa, hasDoc),
			submenu('Crash recovery', [
				item('recovery-one', 'One document', run(() => rehearseRecovery(1))),
				item('recovery-several', 'Several documents', run(() => rehearseRecovery(3)))
			]),
			separator,
			item('clear', 'Clear all', clearRehearsals)
		]
	};
}
