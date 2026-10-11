import { describe, expect, it } from 'vitest';

import { commentKeeper } from './commentField.ts';

/** As much of a textarea as the keeper uses. */
class Field extends EventTarget {
	value = '';
	readOnly = false;
	isConnected = true;
}

/** A field kept for an annotation whose comment is `comment`; writing takes a turn, as Rust does. */
function kept(comment: string | null = '') {
	const field = new Field();
	const state = { comment, written: [] as string[] };
	const keeper = commentKeeper(
		() => state.comment,
		async (text) => {
			state.written.push(text);
			await Promise.resolve();
			state.comment = text;
		}
	);
	const detach = keeper(field as unknown as HTMLTextAreaElement) as () => void;
	return { field, state, detach };
}

const turn = () => new Promise((r) => setTimeout(r));

describe('commentKeeper', () => {
	it('writes the text when the field loses the focus, once', async () => {
		const { field, state } = kept();
		field.value = 'A note';
		field.dispatchEvent(new Event('blur'));
		field.dispatchEvent(new Event('blur'));
		expect(state.written).toEqual(['A note']);
		await turn();
		field.dispatchEvent(new Event('blur'));
		expect(state.written).toEqual(['A note']);
	});

	it('writes nothing for a field that was only looked at', () => {
		const { field, state } = kept('As it was');
		field.value = 'As it was';
		field.dispatchEvent(new Event('blur'));
		expect(state.written).toEqual([]);
	});

	it('writes the text of a field taken away while it has the focus', async () => {
		const { field, state, detach } = kept();
		field.value = 'Typed, then a click on the page';
		field.isConnected = false;
		detach();
		await turn();
		expect(state.written).toEqual(['Typed, then a click on the page']);
	});

	it('writes once when the field loses the focus as it is taken away', async () => {
		const { field, state, detach } = kept();
		field.value = 'A note';
		field.dispatchEvent(new Event('blur'));
		field.isConnected = false;
		detach();
		await turn();
		expect(state.written).toEqual(['A note']);
	});

	it('leaves the typing alone when it is attached again to a field that stays', async () => {
		const { field, state, detach } = kept();
		field.value = 'Still typing';
		detach();
		await turn();
		expect(state.written).toEqual([]);
		field.dispatchEvent(new Event('blur'));
		expect(state.written).toEqual([]);
	});

	it('writes the same text again after the comment changed back', async () => {
		const { field, state } = kept();
		field.value = 'A note';
		field.dispatchEvent(new Event('blur'));
		await turn();
		// Undone.
		state.comment = '';
		field.dispatchEvent(new Event('blur'));
		expect(state.written).toEqual(['A note', 'A note']);
	});

	it('writes nothing for an annotation that is gone, or a field that can’t be typed in', async () => {
		const gone = kept(null);
		gone.field.value = 'A note';
		gone.field.dispatchEvent(new Event('blur'));
		gone.field.isConnected = false;
		gone.detach();
		await turn();
		expect(gone.state.written).toEqual([]);

		const fixed = kept('Shown');
		fixed.field.readOnly = true;
		fixed.field.value = 'Shown\n';
		fixed.field.dispatchEvent(new Event('blur'));
		expect(fixed.state.written).toEqual([]);
	});
});
