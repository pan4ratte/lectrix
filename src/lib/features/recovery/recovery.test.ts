import { describe, expect, it } from 'vitest';

import type { RecoveredDocument } from '#lib/ipc/index.ts';

import { describeWhen, discardQuestion, recoveryQuestion } from './recovery.ts';

const now = new Date(2026, 9, 4, 15, 0).getTime();

function doc(name: string, savedAt: number, exists = true): RecoveredDocument {
	return { slot: `1-2-${name.length}`, name, folder: 'C:\\Docs', savedAt, exists };
}

describe('describeWhen', () => {
	it('says today, yesterday, or the date', () => {
		expect(describeWhen(new Date(2026, 9, 4, 9, 5).getTime(), now)).toMatch(/^today at /);
		expect(describeWhen(new Date(2026, 9, 3, 23, 59).getTime(), now)).toMatch(/^yesterday at /);
		const older = describeWhen(new Date(2026, 9, 1, 8, 0).getTime(), now);
		expect(older).toMatch(/^on .+ at /);
		expect(older).not.toMatch(/today|yesterday/);
	});
});

describe('recoveryQuestion', () => {
	it('names a single document and when its changes are from', () => {
		const q = recoveryQuestion([doc('report.pdf', now - 60_000)], now);
		expect(q.message).toBe('Lectrix closed before you saved your changes to report.pdf.');
		expect(q.detail).toMatch(/^The changes are from today at /);
		expect(q.detail).not.toMatch(/moved or deleted/);
		expect(q.buttons.map((b) => b.id)).toEqual(['restore', 'discard', 'later']);
		expect(q.buttons[0]?.primary).toBe(true);
	});

	it('lists several documents and says when a file is gone', () => {
		const q = recoveryQuestion([doc('a.pdf', now), doc('b.pdf', now - 2 * 86_400_000, false)], now);
		expect(q.message).toBe('Lectrix closed before you saved your changes to 2 documents.');
		expect(q.detail).toMatch(/^a\.pdf \(today at [^)]*\), b\.pdf \(on .+; its file was moved or deleted\)\./);
	});

	it('keeps the changes when the question is dismissed', () => {
		expect(recoveryQuestion([doc('a.pdf', now)], now).cancel).toBe('later');
		expect(discardQuestion(1).cancel).toBe('cancel');
		expect(discardQuestion(3).message).toMatch(/3 documents/);
	});
});
