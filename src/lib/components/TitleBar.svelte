<script lang="ts">
	// Custom title bar: the app's icon and name, the menus (not on macOS, where they are in the
	// system's menu bar), the document tabs, a drag region, and the window buttons, placed as
	// the platform's own apps have them (section 8): drawn by the app at the right on Windows;
	// at the end and in the order the desktop gives on Linux (GNOME: close alone, at the
	// right); the system's own at the left on macOS, where the bar leaves room for them. The
	// side panes' buttons are in their own rows (PaneToggle).
	import { Plus } from '@lucide/svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { onMount } from 'svelte';

	import iconUrl from '#lib/assets/lectrix-icon.svg';
	import { commands } from '#lib/commands.ts';
	import { APP_NAME } from '#lib/config.ts';
	import { getWindowChrome, type WindowChrome } from '#lib/ipc/index.ts';
	import { PLATFORM } from '#lib/platform.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import MenuBar from './MenuBar.svelte';
	import Tabs from './Tabs.svelte';
	import WindowControls from './WindowControls.svelte';

	/** Until Rust says: Windows' buttons on Windows, none elsewhere (none drawn rather than
	 * the wrong ones, for the moment it takes). */
	let chrome = $state<WindowChrome>(
		PLATFORM === 'windows' ? { left: [], right: ['minimize', 'maximize', 'close'] } : { left: [], right: [] }
	);
	/** macOS: the window is in a full-screen space of its own (its green button), where the
	 * system hides its buttons, so the bar needs no room for them. */
	let nativeFullscreen = $state(false);

	onMount(() => {
		void getWindowChrome()
			.then((c) => (chrome = c))
			.catch(() => {});
		if (PLATFORM !== 'macos') return;
		const appWindow = getCurrentWindow();
		const follow = () =>
			void appWindow
				.isFullscreen()
				.then((f) => (nativeFullscreen = f))
				.catch(() => {});
		follow();
		const unlisten = appWindow.onResized(follow);
		return () => void unlisten.then((f) => f());
	});

	/** Something is open, so the start screen can be shown over it (and left again). */
	const canShowStart = $derived(app.tabs.length > 0 || app.combine !== null);

	// The icon and name are a button (the start screen), and still move the window when
	// dragged, as the rest of the bar does: a press that moves becomes a window drag.
	const DRAG_AFTER = 4;
	let press: { x: number; y: number } | null = null;
	let dragged = false;

	function onPointerDown(event: PointerEvent) {
		dragged = false;
		press = event.button === 0 ? { x: event.screenX, y: event.screenY } : null;
	}

	function onPointerMove(event: PointerEvent) {
		if (press && Math.hypot(event.screenX - press.x, event.screenY - press.y) >= DRAG_AFTER) dragWindow();
	}

	function dragWindow() {
		if (!press) return;
		press = null;
		// The click that may follow the drag is not one.
		dragged = true;
		void getCurrentWindow()
			.startDragging()
			.catch(() => {});
	}

	function onClick() {
		if (dragged) dragged = false;
		else app.toggleStart();
	}
</script>

<header class="title-bar flex h-[35px] shrink-0 items-stretch bg-chrome" data-tauri-drag-region>
	<!-- The bar is 35 px tall, as VS Code's. The app's icon and name are measured as there too:
	     the 16 px icon centred in a 35 px box, the name in 12 px like its window title. -->
	{#if PLATFORM === 'macos' && !nativeFullscreen}
		<!-- Under the system's buttons (placed in windows.rs: 13 px in, 20 px apart, 14 px
		     wide), with as much room after them as before. -->
		<div class="w-[80px] shrink-0" data-tauri-drag-region></div>
	{/if}
	{#if chrome.left.length > 0}
		<WindowControls buttons={chrome.left} side="left" />
	{/if}
	<button
		type="button"
		class="app-home flex shrink-0 items-center pr-[8px] select-none"
		aria-label="{APP_NAME}: start screen"
		data-tip={canShowStart ? (app.startShown ? 'Back to where you were' : 'Start screen') : undefined}
		aria-pressed={canShowStart ? app.startShown : undefined}
		aria-disabled={!canShowStart}
		tabindex={canShowStart ? 0 : -1}
		onpointerdown={onPointerDown}
		onpointermove={onPointerMove}
		onpointerleave={dragWindow}
		onpointerup={() => (press = null)}
		onpointercancel={() => (press = null)}
		onclick={onClick}
	>
		<span class="flex w-[35px] justify-center">
			<img src={iconUrl} alt="" width="16" height="16" draggable="false" />
		</span>
		<span class="text-[12px]">{APP_NAME}</span>
	</button>
	<!-- macOS has the menus in the system's menu bar (NativeMenus). -->
	{#if PLATFORM !== 'macos'}
		<div class="flex items-center">
			<MenuBar />
		</div>
	{/if}
	<div class="ml-2 flex min-w-0 flex-1 items-center" data-tauri-drag-region>
		<Tabs />
		<!-- After the last tab, as a browser's: File > Open. -->
		<button
			type="button"
			class="icon-button tab-new ml-0.5 shrink-0"
			aria-label="Open file"
			data-tip={app.hint('Open file', 'open')}
			data-open-tab
			onclick={commands.open}
		>
			<Plus size={16} aria-hidden="true" />
		</button>
		<div class="h-full min-w-12 flex-1" data-tauri-drag-region></div>
	</div>
	{#if chrome.right.length > 0}
		<WindowControls buttons={chrome.right} side="right" />
	{/if}
</header>

<style>
	/* The app's icon and name: the way to the start screen while something is open (a fill
	   under the pointer, and none once the start screen shows: there it is where one is, not
	   something being pointed at); with nothing open, only a place to drag the window by. */
	.app-home {
		border: 0;
		background: transparent;
	}
	.app-home:not([aria-disabled='true'], [aria-pressed='true']):hover {
		background: var(--lectrix-hover);
	}
	.app-home:not([aria-disabled='true'], [aria-pressed='true']):active {
		background: var(--lectrix-pressed);
	}
	.app-home:focus-visible {
		outline-offset: -2px;
	}
</style>
