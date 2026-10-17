<script lang="ts">
	// Minimize, maximize/restore and close, drawn by the app because the window has no
	// system title bar (section 8): on Windows as Windows draws them, at the right; on Linux as
	// GTK's title bars (Adwaita) draw them, the buttons and the end the desktop gives
	// (TitleBar). macOS draws its own.
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { onMount } from 'svelte';

	import type { WindowButton } from '#lib/ipc/index.ts';
	import { PLATFORM } from '#lib/platform.ts';

	let { buttons, side }: { buttons: readonly WindowButton[]; side: 'left' | 'right' } = $props();

	const appWindow = getCurrentWindow();
	const gtk = PLATFORM === 'linux';
	let maximized = $state(false);

	onMount(() => {
		void appWindow.isMaximized().then((m) => (maximized = m));
		const unlisten = appWindow.onResized(async () => {
			maximized = await appWindow.isMaximized();
		});
		return () => void unlisten.then((f) => f());
	});

	const NAMES = { minimize: 'Minimize', close: 'Close' } as const;
	const nameOf = (button: WindowButton) => (button === 'maximize' ? (maximized ? 'Restore' : 'Maximize') : NAMES[button]);

	function press(button: WindowButton) {
		if (button === 'minimize') void appWindow.minimize();
		else if (button === 'maximize') void appWindow.toggleMaximize();
		else void appWindow.close();
	}
</script>

<div class={['flex h-full', gtk ? ['gtk-controls items-center', side === 'left' ? 'pl-[6px]' : 'pr-[6px]'] : 'items-stretch']}>
	{#each buttons as button (button)}
		<button
			type="button"
			class={gtk ? 'gtk-button' : 'window-button'}
			class:window-close={!gtk && button === 'close'}
			aria-label={nameOf(button)}
			data-tip={gtk ? undefined : nameOf(button)}
			onclick={() => press(button)}
		>
			{#if gtk}
				<!-- Adwaita's symbolic icons, at their size in its title bars. -->
				<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5">
					{#if button === 'minimize'}
						<path d="M4.5 10.75h7" stroke-linecap="round" />
					{:else if button === 'maximize' && maximized}
						<rect x="5" y="5" width="6" height="6" rx="1" />
					{:else if button === 'maximize'}
						<rect x="4.25" y="4.25" width="7.5" height="7.5" rx="1.25" />
					{:else}
						<path d="M5 5l6 6M11 5l-6 6" stroke-linecap="round" />
					{/if}
				</svg>
			{:else if button === 'minimize'}
				<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" stroke="currentColor" /></svg>
			{:else if button === 'maximize' && maximized}
				<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor">
					<path d="M2.5 0.5h7v7M0.5 2.5h7v7h-7z" />
				</svg>
			{:else if button === 'maximize'}
				<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor">
					<rect x="0.5" y="0.5" width="9" height="9" />
				</svg>
			{:else}
				<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" /></svg>
			{/if}
		</button>
	{/each}
</div>

<style>
	/* The arrow on window buttons, as the system's own (not the hand of other buttons). */
	.window-button,
	.gtk-button {
		border: 0;
		cursor: default;
	}
	.window-button {
		display: inline-flex;
		width: 46px;
		align-items: center;
		justify-content: center;
		background: transparent;
	}
	.window-button:hover {
		background: var(--lectrix-hover);
	}
	.window-close:hover {
		background: var(--lectrix-close-hover);
		color: var(--lectrix-close-hover-fg);
	}
	/* Adwaita: round buttons 24 px across, 6 px apart, each with a faint fill of the text's
	   colour that deepens under the pointer and when pressed; close is drawn as the others. */
	.gtk-controls {
		gap: 6px;
	}
	.gtk-button {
		display: inline-flex;
		width: 24px;
		height: 24px;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		background: color-mix(in srgb, currentColor 10%, transparent);
		color: var(--lectrix-fg);
	}
	.gtk-button:hover {
		background: color-mix(in srgb, currentColor 15%, transparent);
	}
	.gtk-button:active {
		background: color-mix(in srgb, currentColor 30%, transparent);
	}
</style>
