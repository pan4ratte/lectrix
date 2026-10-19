// Compress (section 6.11, ADR 0027), from the start screen or File > Compress…: asks for a
// PDF (or takes the document shown, saved first), for its password if it has one, then
// shows the Compress dialog, which compresses it into a file of Lectrix's own and saves
// that as the user chooses. Rust holds the file and the result between the steps; the
// webview never names a path.

import {
	compressCancel,
	compressDocument as compressDocumentStep,
	compressPassword,
	compressPick,
	compressSave,
	toAppError,
	type CompressStep
} from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { compressedMessage, saveFirstQuestion } from './compress.ts';

export async function compressFile() {
	try {
		await advance(await compressPick());
	} catch (e) {
		app.showError(toAppError(e));
	}
}

/** Compress for the document in `tab`: its file as saved, so unsaved changes (and a
 * document that is no file yet) are saved first. */
export async function compressDocument(tab: DocTab) {
	if (tab.state.dirty) {
		if ((await app.ask(saveFirstQuestion(tab.name))) !== 'save') return;
		if ((await app.save(tab)) === null) return;
	}
	try {
		await advance(await compressDocumentStep(tab.id));
	} catch (e) {
		app.showError(toAppError(e));
	}
}

async function advance(step: CompressStep) {
	switch (step.kind) {
		case 'cancelled':
			return;
		case 'needsPassword':
			app.askPassword(step.token, step.name, step.retry, 'compress', (password) => answer(step.token, password));
			return;
		case 'ready':
			app.compressRequest = step;
			return;
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
		if (password === null) await compressCancel(token);
		else await advance(await compressPassword(token, password));
	} catch (e) {
		app.showError(toAppError(e));
	}
}

/** Saves the result waiting under `token`, over the file or (`copy`) as a copy the Save
 * dialog names, and opens it. False if the Save dialog was given up: the result still
 * waits. */
export async function saveCompressed(token: number, copy: boolean): Promise<boolean> {
	const saved = await compressSave(token, copy);
	if (saved.kind === 'cancelled') return false;
	// Its file was open in a tab: that tab shows it as read again.
	if (saved.reloaded && saved.opened.kind === 'alreadyOpen') {
		const id = saved.opened.id;
		const open = app.tabs.find((t) => t.id === id);
		if (open) app.showReloaded(open, saved.reloaded);
	}
	app.handleOpenResults([saved.opened]);
	app.notify(compressedMessage(saved.name, saved.replaced, saved.before, saved.after));
	if (saved.pdfaRemoved) app.notifyPdfaRemoved(saved.name, saved.pdfaRemoved, 'compression');
	return true;
}
