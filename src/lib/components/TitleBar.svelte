<script lang="ts">
	// Custom title bar (window decorations are off): sidebar toggle, menus, document tabs,
	// a drag region, and the window buttons.
	import { PanelLeft } from '@lucide/svelte';

	import { commands } from '#lib/commands.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import MenuBar from './MenuBar.svelte';
	import Tabs from './Tabs.svelte';
	import WindowControls from './WindowControls.svelte';
</script>

<header class="flex h-10 shrink-0 items-stretch bg-chrome" data-tauri-drag-region>
	<div class="flex items-center gap-1 pl-1">
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
	<WindowControls />
</header>
