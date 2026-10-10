// The first start after an update (section 6.6). On the start screen, a block beside the
// app's name says so, with a button to What's new; with documents open, a notification
// does. Either stays until dismissed or followed.
import { getVersion } from '@tauri-apps/api/app';

import { APP_NAME } from '#lib/config.ts';
import { app } from '#lib/stores/app.svelte.ts';

export function openWhatsNew() {
	app.whatsNewUpdate = null;
	app.whatsNewOpen = true;
}

/** Says Lectrix was updated to `version` ('' if unknown): beside the name on the start
 * screen, or in a notification. */
export function showUpdated(version: string, where: 'start screen' | 'notification') {
	if (where === 'start screen') {
		app.updatedTo = version;
		return;
	}
	app.notify(
		{
			kind: 'info',
			message: version ? `${APP_NAME} was updated to ${version}` : `${APP_NAME} was updated`,
			suggestion: 'See what’s new in this version.',
			action: { label: 'What’s new', run: openWhatsNew }
		},
		0
	);
}

export async function announceUpdate() {
	const version = await getVersion().catch(() => '');
	showUpdated(version, app.tabs.length === 0 && !app.combine ? 'start screen' : 'notification');
}
