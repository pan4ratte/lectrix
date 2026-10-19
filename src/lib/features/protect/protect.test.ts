import { describe, expect, it } from 'vitest';

import type { ProtectionInfo } from '#lib/ipc/index.ts';

import {
	draftInput,
	draftProblems,
	initialDraft,
	pendingKind,
	protectedMessage,
	protectionKind,
	saveQuestion,
	type ProtectDraft
} from './protect.ts';

function draft(changes: Partial<ProtectDraft> = {}): ProtectDraft {
	return { ...initialDraft(null, null), ...changes };
}

const restricted: ProtectionInfo = {
	openPassword: true,
	restricted: true,
	allowed: {
		print: true,
		printHighResolution: false,
		modify: false,
		copy: false,
		annotate: true,
		fillForms: true,
		assemble: false
	},
	restrictions: { printing: 'lowResolution', changes: 'commentsFormsAndSigning', copying: false },
	cipher: 'aes',
	keyBits: 256,
	unrestricted: false
};

describe('initialDraft', () => {
	it('starts with nothing ticked, high-resolution printing, no changes and copying allowed', () => {
		const d = initialDraft(null, null);
		expect([d.requireOpen, d.restrict]).toEqual([false, false]);
		expect(d.restrictions).toEqual({ printing: 'highResolution', changes: 'none', copying: true });
	});

	it('fills in what a protected file has, but not its passwords', () => {
		const d = initialDraft(restricted, null);
		expect([d.requireOpen, d.restrict]).toEqual([true, true]);
		expect(d.restrictions).toEqual(restricted.restrictions);
		expect(d.openPassword).toBe('');
	});

	it('fills in a change not saved yet over what the file has', () => {
		const d = initialDraft(restricted, { kind: 'set', openPassword: false, restrictions: null });
		expect([d.requireOpen, d.restrict]).toEqual([false, false]);
		expect([initialDraft(restricted, { kind: 'remove' }).requireOpen]).toEqual([false]);
	});
});

describe('draftInput', () => {
	it('needs something ticked, and each ticked password typed twice the same', () => {
		expect(draftInput(draft())).toBeNull();
		expect(draftInput(draft({ requireOpen: true }))).toBeNull();
		expect(draftInput(draft({ requireOpen: true, openPassword: 'a', openConfirm: '' }))).toBeNull();
		expect(draftInput(draft({ requireOpen: true, openPassword: 'a', openConfirm: 'b' }))).toBeNull();
		expect(draftInput(draft({ requireOpen: true, openPassword: 'a', openConfirm: 'a' }))).toEqual({
			openPassword: 'a',
			permissionsPassword: null,
			restrictions: { printing: 'highResolution', changes: 'none', copying: true }
		});
	});

	it('requires a permissions password with restrictions, different from the one to open', () => {
		const both = { requireOpen: true, openPassword: 'a', openConfirm: 'a', restrict: true };
		expect(draftInput(draft(both))).toBeNull();
		expect(draftInput(draft({ ...both, permissionsPassword: 'a', permissionsConfirm: 'a' }))).toBeNull();
		expect(draftInput(draft({ ...both, permissionsPassword: 'b', permissionsConfirm: 'b' }))?.permissionsPassword).toBe('b');
		// Restrictions alone need no password to open.
		expect(
			draftInput(draft({ restrict: true, permissionsPassword: 'b', permissionsConfirm: 'b' }))?.openPassword
		).toBeNull();
	});

	it('leaves out what is not ticked', () => {
		const input = draftInput(
			draft({ openPassword: 'ignored', restrict: true, permissionsPassword: 'b', permissionsConfirm: 'b' })
		);
		expect(input?.openPassword).toBeNull();
	});
});

describe('draftProblems', () => {
	it('says why, under the field it is about', () => {
		expect(draftProblems(draft({ requireOpen: true, openPassword: 'a', openConfirm: 'b' })).open).toBe(
			'The passwords don’t match.'
		);
		// Nothing is said while the confirmation is still empty.
		expect(draftProblems(draft({ requireOpen: true, openPassword: 'a' })).open).toBeNull();
		expect(
			draftProblems(
				draft({ requireOpen: true, openPassword: 'a', openConfirm: 'a', restrict: true, permissionsPassword: 'a' })
			).permissions
		).toBe('Use a different password from the one to open the file.');
		// 127 bytes of UTF-8 at most: "é" is two.
		expect(draftProblems(draft({ requireOpen: true, openPassword: 'é'.repeat(64) })).open).toBe(
			'This password is too long.'
		);
		expect(draftProblems(draft({ requireOpen: true, openPassword: 'é'.repeat(63) + 'x' })).open).toBeNull();
		// A box not ticked has nothing to say.
		expect(draftProblems(draft({ openPassword: 'a', openConfirm: 'b' })).open).toBeNull();
	});
});

describe('wording', () => {
	it('asks Save, Save as a copy or Cancel, and warns about signatures', () => {
		const q = saveQuestion('report.pdf', false);
		expect(q.title).toBe('Protect report.pdf?');
		expect(q.buttons.map((b) => b.label)).toEqual(['Save', 'Save as a copy', 'Cancel']);
		expect(q.cancel).toBe('cancel');
		expect(q.detail).toBeUndefined();
		expect(saveQuestion('report.pdf', true).detail).toMatch(/signatures will not be valid/);
	});

	it('says what was saved', () => {
		expect(protectedMessage('report.pdf', true).message).toBe('report.pdf is now protected.');
		const copy = protectedMessage('report (protected).pdf', false);
		expect(copy.message).toBe('Saved report (protected).pdf with its protection.');
		expect(copy.suggestion).toMatch(/original/);
	});

	it('names a protection in a few words', () => {
		expect(protectionKind(true, true)).toBe('Password to open and restrictions');
		expect(protectionKind(false, true)).toBe('Restrictions');
		expect(protectionKind(false, false)).toBe('None');
		expect(pendingKind({ kind: 'remove' })).toBe('None');
		expect(pendingKind({ kind: 'set', openPassword: true, restrictions: null })).toBe('Password to open');
	});
});
