<script lang="ts">
	// Custom title bar (window decorations are off): the app's icon and name, sidebar toggle,
	// menus, document tabs, a drag region, the annotation pane toggle, and the window buttons.
	import { PanelLeft, PanelRight } from '@lucide/svelte';

	import iconUrl from '#lib/assets/lectrix-icon.svg';
	import { commands } from '#lib/commands.ts';
	import { APP_NAME } from '#lib/config.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import MenuBar from './MenuBar.svelte';
	import Tabs from './Tabs.svelte';
	import WindowControls from './WindowControls.svelte';

	const tab = $derived(app.active);
	/** Some annotation needs repair: the pane's button carries the badge the list shows. */
	const needing = $derived(tab?.allAnnotations.some((a) => a.problems.length > 0) ?? false);
</script>

<header class="flex h-10 shrink-0 items-stretch bg-chrome" data-tauri-drag-region>
	<!-- The app's icon and name, as a native Windows 11 title bar shows them. They pass pointer
	     events to the drag region beneath, so the window can be dragged by them. -->
	<div class="flex shrink-0 items-center gap-2 pr-2 pl-3 select-none" data-tauri-drag-region>
		<img class="pointer-events-none" src={iconUrl} alt="" width="18" height="18" draggable="false" />
		<span class="pointer-events-none text-xs">{APP_NAME}</span>
	</div>
	<div class="flex items-center gap-1">
		<button
			type="button"
			class="icon-button"
			aria-label={app.sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
			aria-pressed={app.sidebarOpen}
			title="Sidebar"
			onclick={commands.toggleSidebar}
		>
			<PanelLeft size={16} aria-hidden="true" />
		</button>
		<MenuBar />
	</div>
	<div class="ml-2 flex min-w-0 flex-1 items-end" data-tauri-drag-region>
		<Tabs />
		<div class="h-full min-w-12 flex-1" data-tauri-drag-region></div>
	</div>
	{#if tab}
		<div class="flex items-center pr-1">
			<button
				type="button"
				class="icon-button relative"
				aria-label={app.annotationsOpen ? 'Hide annotations' : 'Show annotations'}
				aria-pressed={app.annotationsOpen}
				aria-describedby={needing ? 'annotations-need-repair' : undefined}
				title="Annotations"
				onclick={commands.toggleAnnotations}
			>
				<PanelRight size={16} aria-hidden="true" />
				{#if needing}
					<span class="absolute top-1 right-1 h-2 w-2 rounded-full bg-danger" aria-hidden="true"></span>
					<span id="annotations-need-repair" class="sr-only">Some annotations need repair</span>
				{/if}
			</button>
		</div>
	{/if}
	<WindowControls />
</header>
