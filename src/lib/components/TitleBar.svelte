<script lang="ts">
	// Custom title bar (window decorations are off): the app's icon and name, the menus, the
	// document tabs, a drag region, and the window buttons. The side panes' buttons are in
	// their own rows (PaneToggle).
	import iconUrl from '#lib/assets/lectrix-icon.svg';
	import { APP_NAME } from '#lib/config.ts';

	import MenuBar from './MenuBar.svelte';
	import Tabs from './Tabs.svelte';
	import WindowControls from './WindowControls.svelte';
</script>

<header class="flex h-[35px] shrink-0 items-stretch bg-chrome" data-tauri-drag-region>
	<!-- The bar is 35 px tall, as VS Code's. The app's icon and name are measured as there too:
	     the 16 px icon centred in a 35 px box, the name in 12 px like its window title. They pass pointer events to the drag
	     region beneath, so the window can be dragged by them. -->
	<div class="flex shrink-0 items-center pr-[8px] select-none" data-tauri-drag-region>
		<div class="pointer-events-none flex w-[35px] justify-center">
			<img src={iconUrl} alt="" width="16" height="16" draggable="false" />
		</div>
		<span class="pointer-events-none text-[12px]">{APP_NAME}</span>
	</div>
	<div class="flex items-center">
		<MenuBar />
	</div>
	<div class="ml-2 flex min-w-0 flex-1 items-center" data-tauri-drag-region>
		<Tabs />
		<div class="h-full min-w-12 flex-1" data-tauri-drag-region></div>
	</div>
	<WindowControls />
</header>
