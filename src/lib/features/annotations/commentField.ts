// Keeping what is typed in an annotation's comment field (section 6.5): the comment panel's
// note and the selected row's field in the annotation list. The text is written when the
// field loses the focus, and also when the field goes while it still has it: a click on the
// page deselects the annotation, and the field is taken away before it hears that it lost
// the focus. The field is told which annotation it is for when it is made, because by the
// time it is left the selection is another annotation's, or nobody's.

import type { Attachment } from 'svelte/attachments';

/**
 * Writes the field's text as its annotation's comment when the field is left or removed, if
 * it differs. `stored` is the comment the annotation has now (null once it is gone); `write`
 * changes it, resolving when that is done.
 */
export function commentKeeper(stored: () => string | null, write: (text: string) => Promise<unknown>): Attachment<HTMLTextAreaElement> {
	return (field) => {
		/** The text on its way to Rust, which `stored` doesn't show yet. */
		let sending: string | null = null;

		function commit() {
			const text = field.value;
			const now = stored();
			if (field.readOnly || now === null || text === now || text === sending) return;
			sending = text;
			const done = () => {
				if (sending === text) sending = null;
			};
			write(text).then(done, done);
		}

		field.addEventListener('blur', commit);
		return () => {
			field.removeEventListener('blur', commit);
			// Removed, not attached again while it stays (which leaves the typing alone).
			queueMicrotask(() => {
				if (!field.isConnected) commit();
			});
		};
	};
}
