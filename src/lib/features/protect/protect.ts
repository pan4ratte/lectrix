// What Protect says and checks (section 6.9, ADR 0026): the dialog's choices, why its main
// button is disabled, the question asked before a file is written, and the notifications.

import type {
	Changes,
	PendingProtection,
	Printing,
	ProtectionInfo,
	ProtectionInput,
	Restrictions
} from '#lib/ipc/index.ts';

export const PRINTING: readonly { value: Printing; label: string }[] = [
	{ value: 'notAllowed', label: 'Not allowed' },
	{ value: 'lowResolution', label: 'Low resolution (150 dpi)' },
	{ value: 'highResolution', label: 'High resolution' }
];

export const CHANGES: readonly { value: Changes; label: string }[] = [
	{ value: 'none', label: 'None' },
	{ value: 'pageAssembly', label: 'Inserting, deleting and rotating pages' },
	{ value: 'formsAndSigning', label: 'Filling in forms and signing' },
	{ value: 'commentsFormsAndSigning', label: 'Commenting, filling in forms and signing' },
	{ value: 'anyExceptExtracting', label: 'Any except extracting pages' }
];

/** The longest password, in bytes of UTF-8 (revision 6 uses the first 127). */
export const MAX_PASSWORD_BYTES = 127;

/** What the dialog holds. */
export interface ProtectDraft {
	requireOpen: boolean;
	openPassword: string;
	openConfirm: string;
	restrict: boolean;
	permissionsPassword: string;
	permissionsConfirm: string;
	restrictions: Restrictions;
}

/** A new dialog's choices, or those of a file or document protected already: its
 * checkboxes and restrictions as it has them, its passwords to be typed again. */
export function initialDraft(protection: ProtectionInfo | null, pending: PendingProtection | null): ProtectDraft {
	const draft: ProtectDraft = {
		requireOpen: false,
		openPassword: '',
		openConfirm: '',
		restrict: false,
		permissionsPassword: '',
		permissionsConfirm: '',
		restrictions: { printing: 'highResolution', changes: 'none', copying: true }
	};
	if (pending?.kind === 'set') {
		draft.requireOpen = pending.openPassword;
		draft.restrict = pending.restrictions !== null;
		if (pending.restrictions) draft.restrictions = { ...pending.restrictions };
	} else if (pending?.kind !== 'remove' && protection) {
		draft.requireOpen = protection.openPassword;
		draft.restrict = protection.restricted;
		if (protection.restricted) draft.restrictions = { ...protection.restrictions };
	}
	return draft;
}

const bytes = (text: string) => new TextEncoder().encode(text).length;

/** Why a password field keeps the dialog from protecting, or null. `other` is the other
 * password, which this one must differ from (only the permissions password is compared). */
export function passwordProblem(password: string, confirm: string, other: string | null): string | null {
	if (bytes(password) > MAX_PASSWORD_BYTES) return 'This password is too long.';
	if (password.includes('\0')) return 'This password has a character that can’t be used.';
	if (other !== null && password !== '' && password === other) {
		return 'Use a different password from the one to open the file.';
	}
	if (confirm !== '' && confirm !== password) return 'The passwords don’t match.';
	return null;
}

/** The problems of each part of the dialog, shown under its fields. */
export function draftProblems(draft: ProtectDraft) {
	return {
		open: draft.requireOpen ? passwordProblem(draft.openPassword, draft.openConfirm, null) : null,
		permissions: draft.restrict
			? passwordProblem(
					draft.permissionsPassword,
					draft.permissionsConfirm,
					draft.requireOpen ? draft.openPassword : null
				)
			: null
	};
}

/** What Protect would write, or null while what is ticked is not complete. A draft with
 * nothing ticked gives null too: removing protection is a choice of its own. */
export function draftInput(draft: ProtectDraft): ProtectionInput | null {
	const problems = draftProblems(draft);
	if (problems.open || problems.permissions) return null;
	if (!draft.requireOpen && !draft.restrict) return null;
	const complete = (password: string, confirm: string) => password !== '' && confirm === password;
	if (draft.requireOpen && !complete(draft.openPassword, draft.openConfirm)) return null;
	if (draft.restrict && !complete(draft.permissionsPassword, draft.permissionsConfirm)) return null;
	return {
		openPassword: draft.requireOpen ? draft.openPassword : null,
		permissionsPassword: draft.restrict ? draft.permissionsPassword : null,
		restrictions: { ...draft.restrictions }
	};
}

/** Asked once the protection is chosen, for a file: save it so, save a copy so (the
 * original kept as it is), or nothing. A signed file is written whole either way. */
export function saveQuestion(name: string, signed: boolean) {
	return {
		title: `Protect ${name}?`,
		message: 'Save it with this protection, or save a protected copy and keep the original as it is.',
		detail: signed ? SIGNED_WARNING : undefined,
		buttons: [
			{ id: 'save', label: 'Save', primary: true },
			{ id: 'copy', label: 'Save as a copy' },
			{ id: 'cancel', label: 'Cancel' }
		],
		cancel: 'cancel'
	};
}

export const SIGNED_WARNING =
	'This file is signed. Its signatures will not be valid once it is saved with new protection, because the whole file is rewritten.';

/** Said once the file is written with its protection (and opened in a tab): `replaced`
 * when it was written over the file itself. */
export function protectedMessage(name: string, replaced: boolean) {
	return replaced
		? { kind: 'info' as const, message: `${name} is now protected.` }
		: {
				kind: 'info' as const,
				message: `Saved ${name} with its protection.`,
				suggestion: 'The original file is unchanged.'
			};
}

/** A protection in a few words: "Password to open and restrictions". */
export function protectionKind(openPassword: boolean, restricted: boolean): string {
	if (openPassword && restricted) return 'Password to open and restrictions';
	if (openPassword) return 'Password to open';
	if (restricted) return 'Restrictions';
	return 'None';
}

/** What a waiting protection change makes of the file at the next save, in a few words. */
export function pendingKind(pending: PendingProtection): string {
	return pending.kind === 'remove' ? 'None' : protectionKind(pending.openPassword, pending.restrictions !== null);
}
