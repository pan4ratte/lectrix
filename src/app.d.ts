// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
	namespace App {}
	interface Window {
		/** Set before the page loads when the window starts with documents to open
		 * (`windows::create` in Rust): the start screen waits for them. */
		__LECTRIX_OPENING__?: boolean;
	}
}

export {};
