<script lang="ts">
	// The app window: title bar, sidebar, view bar, page canvas, annotation pane, and the
	// wiring between Rust events, shortcuts and the stores.
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
	import PaneToggle from '#lib/components/PaneToggle.svelte';
	import PanelDragGhost from '#lib/components/PanelDragGhost.svelte';
	import PanelPane from '#lib/components/PanelPane.svelte';
	import { draggedPanel, dropSlot } from '#lib/components/panels.svelte.ts';
	import { LEFT_LIMITS, RIGHT_LIMITS, type PaneSide } from '#lib/components/panes.ts';
	import SidePane from '#lib/components/SidePane.svelte';
	import StartScreen from '#lib/components/StartScreen.svelte';
	import TitleBar from '#lib/components/TitleBar.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import SettingsDialog from '#lib/components/SettingsDialog.svelte';
	import { cancelTextDraft } from '#lib/features/annotations/actions.ts';
	import AnnotationPropertiesDialog from '#lib/features/annotations/AnnotationPropertiesDialog.svelte';
	import AnnotationToolbar from '#lib/features/annotations/AnnotationToolbar.svelte';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import CombineView from '#lib/features/merge/CombineView.svelte';
	import InsertPagesDialog from '#lib/features/merge/InsertPagesDialog.svelte';
	import { update } from '#lib/features/update/update.svelte.ts';
	import { firstPagePainted, runPerf } from '#lib/features/viewer/perfrun.ts';
	import RotatePagesDialog from '#lib/features/viewer/RotatePagesDialog.svelte';
	import ViewBar from '#lib/features/viewer/ViewBar.svelte';
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
	import { applyAppearance } from '#lib/theme.ts';

	const tab = $derived(app.active);

	async function startup() {
		try {
			const info = await appReady();
			app.startup = info;
			app.restorePanes(info.panes);
			void getSettings()
				.then((s) => {
					app.settings = s;
					applyAppearance(s.appearance);
					tools.setRemember(s.rememberAnnotationStyle);
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
			// Once the documents are up, so the check never delays them (ADR 0011).
			void update.check();
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

<!-- Room in the view bar's row for a closed pane's button, which floats at the window's edge
     (paneButton). A pane without panels has none; while a panel is dragged, a place to drop
     it shows there instead. -->
{#snippet paneEnd(side: PaneSide)}
	{#if app.panels[side].length > 0 ? !app.isOpen(side) : draggedPanel() !== null}
		<div class="flex border-b border-line bg-chrome px-[4px]">
			<div class="flex h-[40px] w-[32px] items-center">
				{#if app.panels[side].length === 0}
					<div
						class="pane-drop-slot"
						class:pane-drop-target={dropSlot(side) !== null}
						data-panel-drop={side}
						aria-hidden="true"
					></div>
				{/if}
			</div>
		</div>
	{/if}
{/snippet}

<!-- A pane's button, at the window's edge in the row of the pane's tabs, which leaves room
     for it, and in the same place while the pane is closed (as in Obsidian). It floats over
     both, so it stays still while the pane slides open or closed. Pixel sizes, not spacing
     units (a rem is 14 px here): the pane rows and the view bar are all 40 px tall. -->
{#snippet paneButton(side: PaneSide)}
	{#if tab && app.panels[side].length > 0}
		<div class={['absolute top-[4px] z-20', side === 'left' ? 'left-[4px]' : 'right-[4px]']}>
			<PaneToggle {side} />
		</div>
	{/if}
{/snippet}

<div class="flex h-full flex-col">
	<TitleBar />
	<div class="relative flex min-h-0 flex-1">
		{#if app.combineActive && app.combine}
			<main class="flex min-w-0 flex-1 flex-col bg-canvas">
				<CombineView combine={app.combine} />
			</main>
		{:else}
			{@render paneButton('left')}
			{#if tab && app.panels.left.length > 0}
				<SidePane
					side="left"
					label="Left pane"
					open={app.leftOpen}
					bind:width={app.leftWidth}
					limits={LEFT_LIMITS}
					animate={app.panesRestored}
				>
					<PanelPane side="left" {tab} />
				</SidePane>
			{/if}
			<main class="flex min-w-0 flex-1 flex-col bg-canvas">
				{#if tab}
					<div class="flex shrink-0 items-stretch">
						{@render paneEnd('left')}
						<div class="min-w-0 flex-1">
							{#key tab.id}
								<ViewBar {tab} />
							{/key}
						</div>
						{@render paneEnd('right')}
					</div>
					<FileBanner {tab} />
					{#key tab.id}
						<div class="relative flex min-h-0 flex-1 flex-col">
							<Viewer {tab} />
							{#if app.settings?.toolbarStyle !== 'panel'}
								<AnnotationToolbar {tab} />
							{/if}
							<AnnotationPropertiesDialog {tab} />
							<BookmarkInspector {tab} />
						</div>
					{/key}
				{:else}
					<StartScreen />
				{/if}
			</main>
			{#if tab && app.panels.right.length > 0}
				<SidePane
					side="right"
					label="Right pane"
					open={app.rightOpen}
					bind:width={app.rightWidth}
					limits={RIGHT_LIMITS}
					animate={app.panesRestored}
				>
					<PanelPane side="right" {tab} />
				</SidePane>
			{/if}
			{@render paneButton('right')}
		{/if}
	</div>
	{#if tab}
		<RotatePagesDialog {tab} />
	{/if}
</div>

<InsertPagesDialog />
<SettingsDialog />
<AboutDialog />
<DialogHost />
<PasswordDialog />
<Toasts />
<PanelDragGhost />

{#if app.dropTarget}
	<div
		class="pointer-events-none fixed inset-2 z-40 flex items-center justify-center rounded-panel border-2 border-dashed border-accent bg-surface/80 text-lg"
		aria-hidden="true"
	>
		{app.combineActive ? 'Drop PDFs to add them' : 'Drop PDFs to open them'}
	</div>
{/if}
