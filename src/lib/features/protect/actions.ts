// Protect (section 6.9, ADR 0026). As a tool, from the start screen or File > Protect… with
// no document shown: asks for a PDF, for its password if it has one, then for the
// protection, then whether to save the file so or a copy, and opens what was saved; Rust
// holds the file between the steps, and the webview never names a path. For a document in
// a tab: the protection its next save writes, or none.

import {
	getDocumentProtection,
	protectCancel,
	protectPassword,
	protectPick,
	protectSave,
	setDocumentProtection,
	toAppError,
	type ProtectionChangeInput,
	type ProtectStep
} from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import { protectedMessage, saveQuestion } from './protect.ts';

export async function protectFile() {
	try {
		await advance(await protectPick());
	} catch (e) {
		app.showError(toAppError(e));
	}
}

/** The protection `tab`'s document is saved with next. Its own restrictions don't refuse
 * it: whatever password opened it is enough, as for Unlock (ADR 0025). */
export async function protectDocument(tab: DocTab) {
	if (app.refuseReadOnly(tab)) return;
	let protection = null;
	try {
		protection = await getDocumentProtection(tab.id);
	} catch (e) {
		app.showError(toAppError(e));
		return;
	}
	const pending = tab.state.protection;
	const choice = await app.askProtection({
		name: tab.name,
		protection,
		pending,
		signedWarning: tab.flags.signed,
		canRemove: protection !== null || pending?.kind === 'set'
	});
	if (choice === null) return;
	// Nothing to remove from a file without protection: the change waiting goes.
	const change: ProtectionChangeInput | null =
		choice === 'remove' ? (protection ? { kind: 'remove' } : null) : { kind: 'set', protection: choice };
	try {
		tab.state = await setDocumentProtection(tab.id, change);
	} catch (e) {
		app.showError(toAppError(e));
	}
}

async function advance(step: ProtectStep) {
	switch (step.kind) {
		case 'cancelled':
			return;
		case 'needsPassword':
			app.askPassword(step.token, step.name, step.retry, 'protect', (password) => answer(step.token, password));
			return;
		case 'ready': {
			const choice = await app.askProtection({
				name: step.name,
				protection: step.protection,
				pending: null,
				signedWarning: false,
				canRemove: false
			});
			if (choice === null || choice === 'remove') {
				await protectCancel(step.token);
				return;
			}
			// Asked again when the Save dialog of a copy is given up.
			for (;;) {
				const where = await app.ask(saveQuestion(step.name, step.signed));
				if (where !== 'save' && where !== 'copy') {
					await protectCancel(step.token);
					return;
				}
				const saved = await protectSave(step.token, where === 'copy', choice);
				if (saved.kind === 'saved') {
					// Its file was open in a tab: that tab shows it as read again.
					if (saved.reloaded && saved.opened.kind === 'alreadyOpen') {
						const id = saved.opened.id;
						const open = app.tabs.find((t) => t.id === id);
						if (open) app.showReloaded(open, saved.reloaded);
					}
					app.handleOpenResults([saved.opened]);
					app.notify(protectedMessage(saved.name, saved.replaced));
					if (saved.pdfaRemoved) app.notifyPdfaRemoved(saved.name, saved.pdfaRemoved, true);
					return;
				}
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
		if (password === null) await protectCancel(token);
		else await advance(await protectPassword(token, password));
	} catch (e) {
		app.showError(toAppError(e));
	}
}
