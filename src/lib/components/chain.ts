// Runs a bits-ui trigger's own event handler and then ours, so spreading the trigger's
// props onto an element and adding handlers does not silently replace its behavior.

type Handler<E extends Event> = (event: E) => void;

export function chain<E extends Event>(
	props: Record<string, unknown>,
	name: string,
	ours: Handler<E>
): Handler<E> {
	const theirs = props[name];
	return (event: E) => {
		if (typeof theirs === 'function') (theirs as Handler<E>)(event);
		ours(event);
	};
}
