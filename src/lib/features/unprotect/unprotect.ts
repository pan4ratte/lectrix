// What Unlock says (section 6.8, ADR 0025): the question asked once the file is open, and
// the notifications along the way.

/** Asked once the file is open: save it without its password and restrictions, save a
 * copy so (the original kept as it is), or nothing. A signed file is written whole either
 * way, so its signatures no longer match it. */
export function saveQuestion(name: string, signed: boolean) {
	return {
		title: 'Remove the password?',
		message: `${name} is unlocked. Save it without its password and restrictions, or save an unlocked copy and keep the original as it is.`,
		detail: signed
			? 'This file is signed. Its signatures will not be valid in the file saved without the password, because the whole file is rewritten.'
			: undefined,
		buttons: [
			{ id: 'save', label: 'Save', primary: true },
			{ id: 'copy', label: 'Save as a copy' },
			{ id: 'cancel', label: 'Cancel' }
		],
		cancel: 'cancel'
	};
}

/** Asked before a document with unsaved changes is unlocked: Unlock works on its file. */
export function saveFirstQuestion(name: string) {
	return {
		title: 'Save your changes first?',
		message: `${name} has unsaved changes. Unlocking works on the file as it is saved, so save them first.`,
		buttons: [
			{ id: 'save', label: 'Save', primary: true },
			{ id: 'cancel', label: 'Cancel' }
		],
		cancel: 'cancel'
	};
}

/** Said when the picked file has nothing to remove. */
export function notProtectedMessage(name: string) {
	return {
		kind: 'info' as const,
		message: `${name} has no password or restrictions.`,
		suggestion: 'There is nothing to remove.'
	};
}

/** Said once the file is written without its protection (and opened in a tab):
 * `replaced` when it was written over the protected file itself. */
export function unlockedMessage(name: string, replaced: boolean) {
	return replaced
		? { kind: 'info' as const, message: `${name} no longer has a password or restrictions.` }
		: {
				kind: 'info' as const,
				message: `Saved ${name} without a password or restrictions.`,
				suggestion: 'The original file is unchanged.'
			};
}
