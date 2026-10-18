import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocTab } from '#lib/stores/doc.svelte.ts';

const store = vi.hoisted(() => ({ undo: vi.fn(async () => {}) }));
vi.mock('#lib/stores/app.svelte.ts', () => ({ app: store }));

const { madeForComment, undoMadeForComment } = await import('./actions.ts');

function docTab(revision: number) {
	return {
		state: { revision },
		selectedAnnotation: { page: 2, id: 40 } as { page: number; id: number } | null,
		viewer: { focus: vi.fn() }
	};
}

/** A key pressed in a comment field holding `value`. */
function press(value: string, key = 'z', ctrlKey = true) {
	return {
		key,
		code: key === 'z' ? 'KeyZ' : '',
		ctrlKey,
		metaKey: false,
		altKey: false,
		shiftKey: false,
		currentTarget: { value },
		preventDefault: vi.fn(),
		stopPropagation: vi.fn()
	} as unknown as KeyboardEvent;
}

beforeEach(() => vi.clearAllMocks());

describe('Undo in the comment of an annotation just made for it', () => {
	it('takes the annotation back while the comment is empty', () => {
		const tab = docTab(7);
		madeForComment(tab as unknown as DocTab, 2, 40, 7);
		const event = press('');
		expect(undoMadeForComment(tab as unknown as DocTab, 2, 40, event)).toBe(true);
		expect(store.undo).toHaveBeenCalledWith(tab);
		expect(event.preventDefault).toHaveBeenCalled();
		expect(tab.selectedAnnotation).toBeNull();
		expect(tab.viewer.focus).toHaveBeenCalled();
		// Once: the next Undo there is the field's, or nothing's.
		expect(undoMadeForComment(tab as unknown as DocTab, 2, 40, press(''))).toBe(false);
	});

	it('leaves typed text to the field, which takes it back first', () => {
		const tab = docTab(7);
		madeForComment(tab as unknown as DocTab, 2, 40, 7);
		expect(undoMadeForComment(tab as unknown as DocTab, 2, 40, press('Check'))).toBe(false);
		expect(store.undo).not.toHaveBeenCalled();
		// Emptied again, Undo takes the annotation back.
		expect(undoMadeForComment(tab as unknown as DocTab, 2, 40, press(''))).toBe(true);
	});

	it('does nothing once the document has changed since, or for another annotation', () => {
		const tab = docTab(7);
		madeForComment(tab as unknown as DocTab, 2, 40, 7);
		expect(undoMadeForComment(tab as unknown as DocTab, 2, 41, press(''))).toBe(false);
		tab.state.revision = 8;
		expect(undoMadeForComment(tab as unknown as DocTab, 2, 40, press(''))).toBe(false);
		expect(store.undo).not.toHaveBeenCalled();
	});

	it('answers only to Undo', () => {
		const tab = docTab(7);
		madeForComment(tab as unknown as DocTab, 2, 40, 7);
		expect(undoMadeForComment(tab as unknown as DocTab, 2, 40, press('', 'z', false))).toBe(false);
		expect(undoMadeForComment(tab as unknown as DocTab, 2, 40, press('', 'y'))).toBe(false);
		expect(store.undo).not.toHaveBeenCalled();
	});
});
