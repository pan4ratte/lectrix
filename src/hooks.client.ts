// Unexpected frontend errors go to the app log (section 8); the user sees a plain message.
import type { HandleClientError } from '@sveltejs/kit/hooks';

import { logError } from '#lib/ipc/index.ts';

function describe(error: unknown): string {
	return error instanceof Error ? (error.stack ?? error.message) : String(error);
}

export const handleError: HandleClientError = ({ kind, error }) => {
	void logError(`${kind} error: ${describe(error)}`).catch(() => {});
	return { message: 'Something went wrong. Restart Folio; the app log has details.' };
};

if (typeof window !== 'undefined') {
	window.addEventListener('error', (event) => {
		// Chromium reports this when layout settles over two frames; it is harmless.
		if (String(event.message).startsWith('ResizeObserver loop')) return;
		void logError(`uncaught: ${describe(event.error ?? event.message)}`).catch(() => {});
	});
	window.addEventListener('unhandledrejection', (event) => {
		void logError(`unhandled rejection: ${describe(event.reason)}`).catch(() => {});
	});
}
