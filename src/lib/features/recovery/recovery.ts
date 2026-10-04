// Wording for crash recovery (section 7): the question asked at startup when a crash left
// unsaved changes behind.
import type { RecoveredDocument } from '#lib/ipc/index.ts';

const DAY_MS = 24 * 60 * 60 * 1000;

function startOfDay(ms: number): number {
	const d = new Date(ms);
	d.setHours(0, 0, 0, 0);
	return d.getTime();
}

/** "today at 14:32", "yesterday at 09:05", or "on 2 October at 14:32". */
export function describeWhen(savedAt: number, now: number = Date.now()): string {
	const time = new Date(savedAt).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	const days = Math.round((startOfDay(now) - startOfDay(savedAt)) / DAY_MS);
	if (days <= 0) return `today at ${time}`;
	if (days === 1) return `yesterday at ${time}`;
	const date = new Date(savedAt).toLocaleDateString(undefined, { day: 'numeric', month: 'long' });
	return `on ${date} at ${time}`;
}

function describeOne(doc: RecoveredDocument, now: number): string {
	const moved = doc.exists ? '' : '; its file was moved or deleted';
	return `${doc.name} (${describeWhen(doc.savedAt, now)}${moved})`;
}

export interface RecoveryQuestion {
	title: string;
	message: string;
	detail: string;
	buttons: { id: 'restore' | 'discard' | 'later'; label: string; primary?: boolean }[];
	cancel: 'later';
}

/** The startup question. Escape or closing it answers "Not now": nothing is lost. */
export function recoveryQuestion(found: RecoveredDocument[], now: number = Date.now()): RecoveryQuestion {
	const one = found.length === 1 ? found[0] : undefined;
	return {
		title: 'Restore unsaved changes?',
		message: one
			? `Folio closed before you saved your changes to ${one.name}.`
			: `Folio closed before you saved your changes to ${found.length} documents.`,
		detail: one
			? `The changes are from ${describeWhen(one.savedAt, now)}${one.exists ? '' : ', and the file was moved or deleted since'}. Restoring opens the document with them; save it to keep them.`
			: `${found.map((d) => describeOne(d, now)).join(', ')}. Restoring opens the documents with them; save them to keep them.`,
		buttons: [
			{ id: 'restore', label: 'Restore', primary: true },
			{ id: 'discard', label: 'Discard…' },
			{ id: 'later', label: 'Not now' }
		],
		cancel: 'later'
	};
}

/** Asked before recovered changes are deleted for good. */
export function discardQuestion(count: number) {
	return {
		title: 'Discard the recovered changes?',
		message:
			count === 1
				? 'The unsaved changes will be deleted. The file itself stays as it was last saved.'
				: `The unsaved changes to ${count} documents will be deleted. The files themselves stay as they were last saved.`,
		buttons: [
			{ id: 'discard', label: 'Discard', primary: true },
			{ id: 'cancel', label: 'Cancel' }
		],
		cancel: 'cancel'
	};
}
