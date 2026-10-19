import { describe, expect, it } from 'vitest';

import { notProtectedMessage, saveQuestion, unlockedMessage } from './unprotect.ts';

describe('saveQuestion', () => {
	it('offers Save, Save as a copy and Cancel, and lets Escape cancel', () => {
		const q = saveQuestion('contract.pdf', false);
		expect(q.message).toContain('contract.pdf');
		expect(q.buttons.map((b) => b.label)).toEqual(['Save', 'Save as a copy', 'Cancel']);
		expect(q.buttons.find((b) => b.primary)?.id).toBe('save');
		expect(q.cancel).toBe('cancel');
		expect(q.detail).toBeUndefined();
	});

	it('warns that a signed file’s signatures will not be valid', () => {
		expect(saveQuestion('contract.pdf', true).detail).toMatch(/signatures will not be valid/);
	});
});

describe('messages', () => {
	it('name the file', () => {
		expect(notProtectedMessage('a.pdf').message).toBe('a.pdf has no password or restrictions.');
		const copy = unlockedMessage('a (unlocked).pdf', false);
		expect(copy.message).toBe('Saved a (unlocked).pdf without a password or restrictions.');
		expect(copy.suggestion).toMatch(/original/);
		// Saved over the original: it is not unchanged.
		const same = unlockedMessage('a.pdf', true);
		expect(same.message).toBe('a.pdf no longer has a password or restrictions.');
		expect('suggestion' in same).toBe(false);
	});
});
