<script lang="ts">
	// The app window: title bar, sidebar, page canvas, annotation pane, status bar, and the
	// wiring between Rust events, shortcuts and the stores.
	import { X } from '@lucide/svelte';
	import { getCurrentWebview } from '@tauri-apps/api/webview';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { onMount } from 'svelte';

	import { commands } from '#lib/commands.ts';
	import { isContextMenuKey, openContextMenu } from '#lib/components/contextmenu.ts';
	import AboutDialog from '#lib/components/AboutDialog.svelte';
	import BookmarkInspector from '#lib/features/bookmarks/BookmarkInspector.svelte';
	import DialogHost from '#lib/components/DialogHost.svelte';
	import FileBanner from '#lib/components/FileBanner.svelte';
	import PasswordDialog from '#lib/components/PasswordDialog.svelte';
	import { ANNOTATIONS_LIMITS, SIDEBAR_LIMITS } from '#lib/components/panes.ts';
	import SidePane from '#lib/components/SidePane.svelte';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import StartScreen from '#lib/components/StartScreen.svelte';
	import TitleBar from '#lib/components/TitleBar.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import SettingsDialog from '#lib/components/SettingsDialog.svelte';
	import { cancelTextDraft } from '#lib/features/annotations/actions.ts';
	import AnnotationInspector from '#lib/features/annotations/AnnotationInspector.svelte';
	import AnnotationsPanel from '#lib/features/annotations/AnnotationsPanel.svelte';
	import AnnotationToolbar from '#lib/features/annotations/AnnotationToolbar.svelte';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import CombineView from '#lib/features/merge/CombineView.svelte';
	import InsertPagesDialog from '#lib/features/merge/InsertPagesDialog.svelte';
	import { firstPagePainted, runPerf } from '#lib/features/viewer/perfrun.ts';
	import RotatePagesDialog from '#lib/features/viewer/RotatePagesDialog.svelte';
	import StatusBar from '#lib/features/viewer/StatusBar.svelte';
	import Viewer from '#lib/features/viewer/Viewer.svelte';
	import {
		appReady,
		closeDocument,
		getSettings,
		logMetric,
		onDocumentsOpened,
		onFileChanged,
		onMergeSourcesAdded,
		setImageFormat,
		setPaneLayout,
		toAppError
	} from '#lib/ipc/index.ts';
	import { COMMIT_FIELD_FIRST, commandFor, isBlocked, isBrowserZoomKey, isTextInput } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import { applyAppearance, applyTheme } from '#lib/theme.ts';

	const tab = $derived(app.active);

	async function startup() {
		try {
			const info = await appReady();
			app.startup = info;
			app.restorePanes(info.panes);
			applyTheme(info);
			void getSettings()
				.then((s) => {
					app.settings = s;
					applyAppearance(s.appearance);
				})
				.catch(() => {});
			setImageFormat(info.imageFormat);
			void logMetric('main_to_ready_ms', info.mainToReadyMs);
			void app.refreshRecent();
			// Before the startup files: if one of them is a recovered document, it then
			// switches to the restored tab instead of opening without the changes.
			await app.offerRecovery();
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
		if (!inInput && isContextMenuKey(event) && openContextMenu(event.target)) {
			event.preventDefault();
			return;
		}
		const command = commandFor(event, inInput);
		if (command) {
			event.preventDefault();
			// Saving or closing from a text field: the field commits its edit on blur, and
			// the store waits for that edit before saving (app.settled).
			if (inInput && COMMIT_FIELD_FIRST.has(command)) (event.target as HTMLElement).blur();
			commands[command]();
			return;
		}
		if (event.key === 'Escape' && !inInput && tab) {
			// One step back at a time: what is being drawn, the selection, the search, then
			// the Select tool (section 8: Esc picks Select).
			if (tab.draft) {
				if (tab.draft.kind === 'text') cancelTextDraft(tab);
				else tab.draft = null;
			} else if (tab.selection) tab.selection = null;
			else if (tab.selectedAnnotation) tab.selectedAnnotation = null;
			else if (tools.tool !== 'select') tools.tool = 'select';
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

	// The webview's page zoom is on so touchpad pinches arrive (as Ctrl+wheel); it must never
	// scale the app. Capturing listeners, so fields that stop their keys cannot bypass them.
	// The viewer zooms the document from the same events.
	function blockPageZoomKey(event: KeyboardEvent) {
		if (isBrowserZoomKey(event)) event.preventDefault();
	}

	function blockPageZoomWheel(event: WheelEvent) {
		if (event.ctrlKey) event.preventDefault();
	}

	// Remember the side panes, once a resize has settled.
	let panesTimer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		if (!app.panesRestored) return;
		const panes = app.panes;
		clearTimeout(panesTimer);
		panesTimer = setTimeout(() => void setPaneLayout(panes).catch(() => {}), 500);
	});

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
		window.addEventListener('keydown', blockPageZoomKey, { capture: true });
		window.addEventListener('wheel', blockPageZoomWheel, { capture: true, passive: false });
		void startup();
		const unlisteners = [
			onDocumentsOpened((results) => app.handleOpenResults(results)),
			onMergeSourcesAdded((results) => {
				if (app.combine) {
					app.combine.addOpened(results);
				} else {
					// The Combine view closed while the files were opening.
					for (const r of results) if (r.kind === 'opened') void closeDocument(r.document.id).catch(() => {});
				}
			}),
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
			window.removeEventListener('keydown', blockPageZoomKey, { capture: true });
			window.removeEventListener('wheel', blockPageZoomWheel, { capture: true });
			clearTimeout(rememberTimer);
			clearTimeout(panesTimer);
			for (const u of unlisteners) void u.then((f) => f());
		};
	});
</script>

<svelte:window onkeydown={onKeyDown} oncontextmenu={onContextMenu} />

<div class="flex h-full flex-col">
	<TitleBar />
	<div class="flex min-h-0 flex-1">
		{#if app.combineActive && app.combine}
			<main class="flex min-w-0 flex-1 flex-col bg-canvas">
				<CombineView combine={app.combine} />
			</main>
		{:else}
			{#if tab}
				<SidePane
					side="left"
					label="Sidebar"
					open={app.sidebarOpen}
					bind:width={app.sidebarWidth}
					limits={SIDEBAR_LIMITS}
					animate={app.panesRestored}
				>
					<Sidebar {tab} />
				</SidePane>
			{/if}
			<main class="flex min-w-0 flex-1 flex-col bg-canvas">
				{#if tab}
					{#key tab.id}
						{#if app.settings?.toolbarStyle === 'panel'}
							<AnnotationToolbar {tab} />
						{/if}
					{/key}
					<FileBanner {tab} />
					{#key tab.id}
						<div class="relative flex min-h-0 flex-1 flex-col">
							<Viewer {tab} />
							{#if app.settings?.toolbarStyle !== 'panel'}
								<AnnotationToolbar {tab} />
							{/if}
							<AnnotationInspector {tab} />
							<BookmarkInspector {tab} />
						</div>
					{/key}
				{:else}
					<StartScreen />
				{/if}
			</main>
			{#if tab}
				<SidePane
					side="right"
					label="Annotations"
					open={app.annotationsOpen}
					bind:width={app.annotationsWidth}
					limits={ANNOTATIONS_LIMITS}
					animate={app.panesRestored}
				>
					<div class="flex h-10 shrink-0 items-center gap-1 pr-1 pl-3">
						<h2 class="flex-1 text-sm font-semibold">Annotations</h2>
						<button
							type="button"
							class="icon-button"
							aria-label="Hide annotations"
							title="Hide annotations"
							onclick={commands.toggleAnnotations}
						>
							<X size={16} aria-hidden="true" />
						</button>
					</div>
					<div class="min-h-0 flex-1">
						{#key tab.id}
							<AnnotationsPanel {tab} />
						{/key}
					</div>
				</SidePane>
			{/if}
		{/if}
	</div>
	{#if tab}
		<StatusBar {tab} />
		<RotatePagesDialog {tab} />
	{:else if !app.combineActive}
		<footer class="h-8 shrink-0 border-t border-line bg-chrome"></footer>
	{/if}
</div>

<InsertPagesDialog />
<SettingsDialog />
<AboutDialog />
<DialogHost />
<PasswordDialog />
<Toasts />

{#if app.dropTarget}
	<div
		class="pointer-events-none fixed inset-2 z-40 flex items-center justify-center rounded-panel border-2 border-dashed border-accent bg-surface/80 text-lg"
		aria-hidden="true"
	>
		{app.combineActive ? 'Drop PDFs to add them' : 'Drop PDFs to open them'}
	</div>
{/if}
