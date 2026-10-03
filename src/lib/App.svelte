<script lang="ts">
	// The app window: title bar, sidebar, page canvas, status bar, and the wiring between
	// Rust events, shortcuts and the stores.
	import { getCurrentWebview } from '@tauri-apps/api/webview';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { onMount } from 'svelte';

	import { commands } from '#lib/commands.ts';
	import BookmarkInspector from '#lib/features/bookmarks/BookmarkInspector.svelte';
	import DialogHost from '#lib/components/DialogHost.svelte';
	import FileBanner from '#lib/components/FileBanner.svelte';
	import PasswordDialog from '#lib/components/PasswordDialog.svelte';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import StartScreen from '#lib/components/StartScreen.svelte';
	import TitleBar from '#lib/components/TitleBar.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import { firstPagePainted, runPerf } from '#lib/features/viewer/perfrun.ts';
	import RotatePagesDialog from '#lib/features/viewer/RotatePagesDialog.svelte';
	import StatusBar from '#lib/features/viewer/StatusBar.svelte';
	import Viewer from '#lib/features/viewer/Viewer.svelte';
	import {
		appReady,
		logMetric,
		onDocumentsOpened,
		onFileChanged,
		setImageFormat,
		toAppError
	} from '#lib/ipc/index.ts';
	import { commandFor, isBlocked, isTextInput } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import { applyTheme } from '#lib/theme.ts';

	const tab = $derived(app.active);

	async function startup() {
		try {
			const info = await appReady();
			app.startup = info;
			applyTheme(info);
			setImageFormat(info.imageFormat);
			void logMetric('main_to_ready_ms', info.mainToReadyMs);
			void app.refreshRecent();
			const openStartedAt = performance.now();
			await app.openStartup();
			if (app.tabs.length > 0) {
				await firstPagePainted();
				void logMetric('first_page_visible_ms', performance.now() - openStartedAt);
				if (info.perfMode && app.active) await runPerf(app.active, info.perfScrollOnly);
			}
		} catch (e) {
			app.showError(toAppError(e));
		}
	}

	function onKeyDown(event: KeyboardEvent) {
		if (isBlocked(event)) {
			event.preventDefault();
			return;
		}
		const inInput = isTextInput(event.target);
		const command = commandFor(event, inInput);
		if (command) {
			event.preventDefault();
			commands[command]();
			return;
		}
		if (event.key === 'Escape' && !inInput && tab) {
			if (tab.selection) tab.selection = null;
			else if (tab.search.open) {
				tab.search.open = false;
				tab.search.clear();
			}
		}
	}

	function onContextMenu(event: MouseEvent) {
		// No browser context menu (Reload, Inspect...) outside text fields.
		if (!isTextInput(event.target)) event.preventDefault();
	}

	// Remember where the user is, a moment after they stop moving.
	let rememberTimer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const current = tab;
		if (!current) return;
		void current.currentPage;
		void current.zoom;
		void current.rotation;
		clearTimeout(rememberTimer);
		rememberTimer = setTimeout(() => app.rememberView(current), 1500);
	});

	onMount(() => {
		void startup();
		const unlisteners = [
			onDocumentsOpened((results) => app.handleOpenResults(results)),
			onFileChanged((event) => app.fileChanged(event.id, event.exists)),
			getCurrentWindow().onCloseRequested(async (event) => {
				if (!(await app.confirmExit())) event.preventDefault();
			}),
			getCurrentWebview().onDragDropEvent((event) => {
				const type = event.payload.type;
				app.dropTarget = type === 'enter' || type === 'over';
			})
		];
		return () => {
			clearTimeout(rememberTimer);
			for (const u of unlisteners) void u.then((f) => f());
		};
	});
</script>

<svelte:window onkeydown={onKeyDown} oncontextmenu={onContextMenu} />

<div class="flex h-full flex-col">
	<TitleBar />
	<div class="flex min-h-0 flex-1">
		{#if tab && app.sidebarOpen}
			<Sidebar {tab} />
		{/if}
		<main class="flex min-w-0 flex-1 flex-col bg-canvas">
			{#if tab}
				<FileBanner {tab} />
				{#key tab.id}
					<div class="relative flex min-h-0 flex-1 flex-col">
						<Viewer {tab} />
						<BookmarkInspector {tab} />
					</div>
				{/key}
			{:else}
				<StartScreen />
			{/if}
		</main>
	</div>
	{#if tab}
		<StatusBar {tab} />
		<RotatePagesDialog {tab} />
	{:else}
		<footer class="h-8 shrink-0 border-t border-line bg-chrome"></footer>
	{/if}
</div>

<DialogHost />
<PasswordDialog />
<Toasts />

{#if app.dropTarget}
	<div
		class="pointer-events-none fixed inset-2 z-40 flex items-center justify-center rounded-panel border-2 border-dashed border-accent bg-surface/80 text-lg"
		aria-hidden="true"
	>
		Drop PDFs to open them
	</div>
{/if}
