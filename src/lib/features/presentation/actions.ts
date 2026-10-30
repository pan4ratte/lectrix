// Starting and ending a presentation (section 6.1), for the command.

import { fullscreen } from '#lib/features/fullscreen/fullscreen.svelte.ts';
import { toAppError } from '#lib/ipc/index.ts';
import { isTextInput } from '#lib/shortcuts.ts';
import { app } from '#lib/stores/app.svelte.ts';

import { presentation } from './presentation.svelte.ts';

/** Presents the document on screen from its current page, or ends the presentation. */
export async function togglePresentation() {
	if (!presentation.on) {
		const tab = app.active;
		// A document being read, not its grid of pages being organized; and a window not on
		// its way into full screen or out of it.
		if (!tab || tab.organize || fullscreen.switching) return;
		// A comment or a name being typed in a pane is written when its field is left.
		const field = document.activeElement;
		if (field instanceof HTMLElement && isTextInput(field)) field.blur();
	}
	try {
		await presentation.toggle(app.active);
	} catch (e) {
		app.showError(toAppError(e));
	}
}
