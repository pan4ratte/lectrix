<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';

	import App from '#lib/App.svelte';
	import Audience from '#lib/features/presentation/Audience.svelte';
	import { ownerOf } from '#lib/features/presentation/relay.ts';
	import { logError } from '#lib/ipc/index.ts';

	/** A presentation's audience screen on a second screen is that alone (audience.rs). */
	const audience = ownerOf(getCurrentWindow().label) !== null;

	function report(error: unknown) {
		const text = error instanceof Error ? (error.stack ?? error.message) : String(error);
		void logError(`render error: ${text}`).catch(() => {});
	}
</script>

<!-- A rendering error must not leave a blank window: documents stay open in the engine,
     so "Try again" rebuilds the interface around them. -->
<svelte:boundary onerror={report}>
	{#if audience}
		<Audience />
	{:else}
		<App />
	{/if}
	{#snippet failed(_error, reset)}
		<div class="flex h-full flex-col items-center justify-center gap-4 bg-bg p-8 text-center">
			<p class="text-base font-semibold">Something went wrong in the window.</p>
			<p class="text-fg-muted">Your open documents and unsaved changes are still there.</p>
			<button type="button" class="button button-primary" onclick={reset}>Try again</button>
		</div>
	{/snippet}
</svelte:boundary>
