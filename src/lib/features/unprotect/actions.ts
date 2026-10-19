// Unlock (section 6.8, ADR 0025), from the start screen or File > Unlock…: asks for a PDF,
// for its password if it has one, then whether to save it without its password and
// restrictions or save a copy so, and opens what was saved. From a document whose
// security settings refused a change, the same for its file. Rust holds the file between
// the steps; the webview never names a path.

import {
	toAppError,
	unprotectCancel,
	unprotectDocument,
	unprotectPassword,
	unprotectPick,
	unprotectSave,
	type UnprotectStep
} from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { notProtectedMessage, saveFirstQuestion, saveQuestion, unlockedMessage } from './unprotect.ts';

export async function unlockFile() {
	try {
		await advance(await unprotectPick());
	} catch (e) {
		app.showError(toAppError(e));
	}
}

/** Unlock for the document in `tab`: its file as saved, so unsaved changes are saved
 * first. A file opened with a password needs it no more. */
export async function unlockDocument(tab: DocTab) {
	if (tab.state.dirty) {
		if ((await app.ask(saveFirstQuestion(tab.name))) !== 'save') return;
		if ((await app.save(tab)) === null) return;
	}
	try {
		await advance(await unprotectDocument(tab.id));
	} catch (e) {
		app.showError(toAppError(e));
	}
}

/** Says that `tab`'s security settings refuse a change (`message`), with a button that
 * unlocks it. */
export function refuseRestricted(tab: DocTab, message: string) {
	app.notify({
		kind: 'error',
		message,
		suggestion: 'Unlock it to remove its restrictions.',
		action: { label: 'Unlock…', run: () => void unlockDocument(tab) }
	});
}

async function advance(step: UnprotectStep) {
	switch (step.kind) {
		case 'cancelled':
			return;
		case 'notProtected':
			app.notify(notProtectedMessage(step.name));
			return;
		case 'needsPassword':
			app.askPassword(step.token, step.name, step.retry, 'unlock', (password) =>
				answer(step.token, password)
			);
			return;
		case 'ready':
			// Asked again when the Save dialog of a copy is given up.
			for (;;) {
				const choice = await app.ask(saveQuestion(step.name, step.signed));
				if (choice !== 'save' && choice !== 'copy') {
					await unprotectCancel(step.token);
					return;
				}
				const saved = await unprotectSave(step.token, choice === 'copy');
				if (saved.kind === 'saved') {
					// Its file was open in a tab: that tab shows it as read again.
					if (saved.reloaded && saved.opened.kind === 'alreadyOpen') {
						const id = saved.opened.id;
						const open = app.tabs.find((t) => t.id === id);
						if (open) app.showReloaded(open, saved.reloaded);
					}
					app.handleOpenResults([saved.opened]);
					app.notify(unlockedMessage(saved.name, saved.replaced));
					return;
				}
			}
		case 'failed':
			app.notify(
				{ kind: 'error', message: `${step.name}: ${step.error.message}`, suggestion: step.error.suggestion },
				12000
			);
			return;
	}
}

async function answer(token: number, password: string | null) {
	try {
		if (password === null) await unprotectCancel(token);
		else await advance(await unprotectPassword(token, password));
	} catch (e) {
		app.showError(toAppError(e));
	}
}
