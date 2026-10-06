<script lang="ts">
	// Minimize, maximize/restore and close, drawn by the app because the window has no
	// system title bar (section 8).
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { onMount } from 'svelte';

	const appWindow = getCurrentWindow();
	let maximized = $state(false);

	onMount(() => {
		void appWindow.isMaximized().then((m) => (maximized = m));
		const unlisten = appWindow.onResized(async () => {
			maximized = await appWindow.isMaximized();
		});
		return () => void unlisten.then((f) => f());
	});
</script>

<div class="flex h-full items-stretch">
	<button
		type="button"
		class="window-button"
		aria-label="Minimize"
		title="Minimize"
		onclick={() => void appWindow.minimize()}
	>
		<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" stroke="currentColor" /></svg>
	</button>
	<button
		type="button"
		class="window-button"
		aria-label={maximized ? 'Restore' : 'Maximize'}
		title={maximized ? 'Restore' : 'Maximize'}
		onclick={() => void appWindow.toggleMaximize()}
	>
		{#if maximized}
			<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor">
				<path d="M2.5 0.5h7v7M0.5 2.5h7v7h-7z" />
			</svg>
		{:else}
			<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor">
				<rect x="0.5" y="0.5" width="9" height="9" />
			</svg>
		{/if}
	</button>
	<button
		type="button"
		class="window-button window-close"
		aria-label="Close"
		title="Close"
		onclick={() => void appWindow.close()}
	>
		<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" /></svg>
	</button>
</div>

<style>
	.window-button {
		display: inline-flex;
		width: 46px;
		align-items: center;
		justify-content: center;
		border: 0;
		background: transparent;
		/* The arrow, as Windows' own window buttons (not the hand of other buttons). */
		cursor: default;
	}
	.window-button:hover {
		background: var(--lectrix-hover);
	}
	.window-close:hover {
		background: var(--lectrix-close-hover);
		color: var(--lectrix-close-hover-fg);
	}
</style>
