<script lang="ts">
	// Custom title bar (window decorations are off): the app's icon and name, the menus, the
	// document tabs, a drag region, and the window buttons. The side panes' buttons are in
	// their own rows (PaneToggle).
	import { getCurrentWindow } from '@tauri-apps/api/window';

	import iconUrl from '#lib/assets/lectrix-icon.svg';
	import { APP_NAME } from '#lib/config.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import MenuBar from './MenuBar.svelte';
	import Tabs from './Tabs.svelte';
	import WindowControls from './WindowControls.svelte';

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

<header class="flex h-[35px] shrink-0 items-stretch bg-chrome" data-tauri-drag-region>
	<!-- The bar is 35 px tall, as VS Code's. The app's icon and name are measured as there too:
	     the 16 px icon centred in a 35 px box, the name in 12 px like its window title. -->
	<button
		type="button"
		class="app-home flex shrink-0 items-center pr-[8px] select-none"
		aria-label="{APP_NAME}: start screen"
		title={canShowStart ? (app.startShown ? 'Back to where you were' : 'Start screen') : undefined}
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
	<div class="flex items-center">
		<MenuBar />
	</div>
	<div class="ml-2 flex min-w-0 flex-1 items-center" data-tauri-drag-region>
		<Tabs />
		<div class="h-full min-w-12 flex-1" data-tauri-drag-region></div>
	</div>
	<WindowControls />
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
