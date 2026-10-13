// What a dialog goes on showing while it closes (section 8): a dialog fades out for a
// moment after what it showed is gone (the question was answered, the request dropped), and
// must not empty or change on the way.

/**
 * The last value `read` gave that was not null. What acts on the value (a button, a form)
 * uses the value itself, which is null once the dialog closes.
 */
export function held<T>(read: () => T | null): { readonly current: T | null } {
	let last: T | null = null;
	const value = $derived.by(() => {
		const now = read();
		if (now !== null) last = now;
		return last;
	});
	return {
		get current() {
			return value;
		}
	};
}
