<script lang="ts">
	import App from '#lib/App.svelte';
	import { logError } from '#lib/ipc/index.ts';

	function report(error: unknown) {
		const text = error instanceof Error ? (error.stack ?? error.message) : String(error);
		void logError(`render error: ${text}`).catch(() => {});
	}
</script>

<!-- A rendering error must not leave a blank window: documents stay open in the engine,
     so "Try again" rebuilds the interface around them. -->
<svelte:boundary onerror={report}>
	<App />
	{#snippet failed(_error, reset)}
		<div class="flex h-full flex-col items-center justify-center gap-4 bg-bg p-8 text-center">
			<p class="text-base font-semibold">Something went wrong in the window.</p>
			<p class="text-fg-muted">Your open documents and unsaved changes are still there.</p>
			<button type="button" class="button button-primary" onclick={reset}>Try again</button>
		</div>
	{/snippet}
</svelte:boundary>
