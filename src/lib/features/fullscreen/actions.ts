// Starting and leaving full screen (section 6.1), for the command and the toolbar's button.

import { toAppError } from '#lib/ipc/index.ts';
import { isTextInput } from '#lib/shortcuts.ts';
import { app } from '#lib/stores/app.svelte.ts';

import { fullscreen } from './fullscreen.svelte.ts';

let hinted = false;

/** How to leave full screen and reach the panes, said the first time it starts while
 * Lectrix runs (and by the Debug menu). */
export function hintFullScreen() {
	const keys = app.shortcut('fullScreen');
	app.notify(
		{
			kind: 'info',
			message: `Press ${keys ? `${keys} or Esc` : 'Esc'} to leave full screen.`,
			suggestion: 'Hold Alt to see the side panes and the toolbar, or move the pointer to a side of the screen.'
		},
		8000
	);
}

/** Full screen shows a document and nothing else, so it starts only with one on screen. */
export async function toggleFullScreen() {
	if (!fullscreen.on && !app.active) return;
	// The panes go, and a comment or a name being typed in one is written when its field is left.
	const field = document.activeElement;
	if (field instanceof HTMLElement && isTextInput(field)) field.blur();
	try {
		await fullscreen.toggle();
	} catch (e) {
		app.showError(toAppError(e));
		return;
	}
	if (fullscreen.on && !hinted) {
		hinted = true;
		hintFullScreen();
	}
	// What had the focus (a menu, the toolbar's button) may be gone: the keys go to the pages.
	app.active?.viewer?.focus();
}
