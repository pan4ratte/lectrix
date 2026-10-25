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
	import PdfaBanner from '#lib/components/PdfaBanner.svelte';
	import PasswordDialog from '#lib/components/PasswordDialog.svelte';
	import NativeMenus from '#lib/components/NativeMenus.svelte';
	import PaneToggle from '#lib/components/PaneToggle.svelte';
	import PanelDragGhost from '#lib/components/PanelDragGhost.svelte';
	import PanelPane from '#lib/components/PanelPane.svelte';
	import { draggedPanel, dropSlot } from '#lib/components/panels.svelte.ts';
	import { LEFT_LIMITS, RIGHT_LIMITS, shownWidth, type PaneSide } from '#lib/components/panes.ts';
	import SidePane from '#lib/components/SidePane.svelte';
	import StartScreen from '#lib/components/StartScreen.svelte';
	import TitleBar from '#lib/components/TitleBar.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import { cancelTextDraft } from '#lib/features/annotations/actions.ts';
	import AnnotationPropertiesDialog from '#lib/features/annotations/AnnotationPropertiesDialog.svelte';
	import AnnotationToolbar from '#lib/features/annotations/AnnotationToolbar.svelte';
	import { announceUpdate } from '#lib/features/changelog/announce.ts';
	import { APP_NAME } from '#lib/config.ts';
	import { tools } from '#lib/features/annotations/state.svelte.ts';
	import { toggleFullScreen } from '#lib/features/fullscreen/actions.ts';
	import { fullscreen } from '#lib/features/fullscreen/fullscreen.svelte.ts';
	import { applySettings } from '#lib/features/settings/apply.ts';
	import CombineView from '#lib/features/merge/CombineView.svelte';
	import PageLabelsDialog from '#lib/features/labels/PageLabelsDialog.svelte';
	import PropertiesDialog from '#lib/features/properties/PropertiesDialog.svelte';
	import ProtectDialog from '#lib/features/protect/ProtectDialog.svelte';
	import CompressDialog from '#lib/features/compress/CompressDialog.svelte';
	import PrintDialog from '#lib/features/print/PrintDialog.svelte';
	import InsertPagesDialog from '#lib/features/merge/InsertPagesDialog.svelte';
	import SettingsDialog from '#lib/features/settings/SettingsDialog.svelte';
	import { update } from '#lib/features/update/update.svelte.ts';
	import { firstPagePainted, runPerf, watchPerf } from '#lib/features/viewer/perfrun.ts';
	import { dispatchPinch } from '#lib/features/viewer/pinch.ts';
	import { insertDropped, leaveOrganize } from '#lib/features/organize/actions.ts';
	import OrganizeBar from '#lib/features/organize/OrganizeBar.svelte';
	import OrganizeView from '#lib/features/organize/OrganizeView.svelte';
	import DropPreview from '#lib/features/split/DropPreview.svelte';
	import SplitView from '#lib/features/split/SplitView.svelte';
	import { dropTarget } from '#lib/features/split/tabdrop.svelte.ts';
	import ViewBar from '#lib/features/viewer/ViewBar.svelte';
	import Viewer from '#lib/features/viewer/Viewer.svelte';
	import WhatsNewDialog from '#lib/features/changelog/WhatsNewDialog.svelte';
	import {
		appReady,
		closeDeclined,
		getSettings,
		logMetric,
		onDocumentSaved,
		onDocumentsMoved,
		onDocumentsOpened,
		onFileChanged,
		onMergeSourcesAdded,
		onRecentChanged,
		onSettingsChanged,
		onShowDocument,
		onTouchpadPinch,
		setImageFormat,
		setPaneLayout,
		toAppError
	} from '#lib/ipc/index.ts';
	import { PLATFORM } from '#lib/platform.ts';
	import { COMMIT_FIELD_FIRST, commandFor, isBlocked, isBrowserZoomKey, isTextInput } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	// The document shown is read from `app.active` each time, never kept in a $derived
	// here: Svelte 5.57 leaves a $derived stale for good when a closing component's
	// cleanup reads it just as another tab is shown (stores/app.svelte.ts).

	async function startup() {
		try {
			const info = await appReady();
			app.startup = info;
			app.restorePanes(info.panes);
			const settings = getSettings()
				.then(applySettings)
				.catch(() => {});
			setImageFormat(info.imageFormat);
			// A window made for a tab moved out of another (section 8) only shows that tab:
			// Lectrix has started already.
			const first = info.firstWindow;
			if (first) void logMetric('main_to_ready_ms', info.mainToReadyMs);
			void app.refreshRecent();
			// Before the startup files: if one of them is a recovered document, it then
			// switches to the restored tab instead of opening without the changes.
			if (first) await app.offerRecovery();
			// Tabs open as Settings say (the page display).
			await settings;
			const openStartedAt = performance.now();
			await app.openStartup();
			if (first && app.tabs.length > 0) {
				await firstPagePainted();
				void logMetric('first_page_visible_ms', performance.now() - openStartedAt);
				if (info.perfWatch) await watchPerf();
				else if (info.perfMode && app.active) await runPerf(app.active, info.perfScrollOnly);
			}
			// The first start after an update offers what changed (section 6.6).
			if (first && info.updated) void announceUpdate();
			// Once the documents are up, so the check never delays them (ADR 0011).
			update.startChecks();
		} catch (e) {
			app.showError(toAppError(e));
		}
	}

	function onKeyDown(event: KeyboardEvent) {
		const inInput = isTextInput(event.target);
		const command = commandFor(event, inInput);
		// A browser key (print, view source...) does nothing, unless Settings made it a shortcut.
		if (!command && isBlocked(event)) {
			event.preventDefault();
			return;
		}
		if (!inInput && isContextMenuKey(event) && openContextMenu(event.target)) {
			event.preventDefault();
			return;
		}
		if (command) {
			event.preventDefault();
			// Saving or closing from a text field: the field commits its edit on blur, and
			// the store waits for that edit before saving (app.settled).
			if (inInput && COMMIT_FIELD_FIRST.has(command)) (event.target as HTMLElement).blur();
			commands[command]();
			return;
		}
		const tab = app.active;
		if (event.key === 'Escape' && !inInput && tab?.organize) {
			// An Esc that closes a menu or a dialog does that and nothing more.
			if (event.target instanceof Element && event.target.closest('[role=menu], [role=dialog], [role=alertdialog]')) return;
			// Back to reading. (A selection in the grid goes first: the grid keeps that Esc.)
			leaveOrganize(tab);
			return;
		}
		if (event.key === 'Escape' && !inInput && tab) {
			// An Esc that closes a menu or a dialog does that and nothing more: the page's
			// context menu leaves the selected text or annotation it was opened on.
			if (event.target instanceof Element && event.target.closest('[role=menu], [role=dialog], [role=alertdialog]')) return;
			// One step back at a time: what is being drawn, the selection, the search, then
			// the Select tool (section 8: Esc picks Select).
			if (tab.draft) {
				if (tab.draft.kind === 'text') cancelTextDraft(tab);
				else tab.draft = null;
			} else if (tab.selection || tab.caret) {
				tab.selection = null;
				tab.caret = null;
			} else if (tab.selectedAnnotation) tab.selectedAnnotation = null;
			else if (tools.tool !== 'select') tools.tool = 'select';
			else if (tab.search.open) {
				tab.search.open = false;
				tab.search.clear();
			} else if (fullscreen.on) void toggleFullScreen();
		}
	}

	// ----- full screen (section 6.1) -----

	// The window fills the screen and shows the pages alone: the title bar and the view bar
	// slide away, the side panes lie over the pages when something brings them (the pointer
	// at a side of the screen, a command, Alt held), and the toolbar floats.
	//
	// It comes and goes the same way (`Stage` in fullscreen.svelte.ts): the bars and the
	// panes slide, laid out as in a window, over pages that have the whole window or screen
	// meanwhile (`viewer-covers`), so no gap opens where a bar was and the pages never wait
	// for the slide; then the window is switched, and only then is everything laid out for
	// its new size, once. They slide by transforms alone (`chrome-slide`, `chrome-away` in
	// app.css), which lays nothing out.
	const full = $derived(fullscreen.on);
	/** The pages are being organized (section 6.7): the grid has the whole window under the
	 * tabs, and the side panes, which are for reading, are not there. */
	const organizing = () => Boolean(app.active?.organize);
	const sliding = $derived(fullscreen.stage === 'arriving' || fullscreen.stage === 'returning');
	const away = $derived(sliding && fullscreen.chromeAway);
	/** The annotation toolbar floats over the pages: Settings say so, or full screen does. */
	const floatingTools = $derived(app.settings !== null && (app.settings.toolbarStyle === 'floating' || fullscreen.started));
	/** The side of the pages the annotation toolbar is docked at, in a bar of its own
	 * (docked at the top, it is in the view bar). */
	const toolDockSide = $derived(
		app.settings?.toolbarStyle === 'panel' && app.settings.toolbarDock !== 'top' && !fullscreen.started
			? app.settings.toolbarDock
			: null
	);
	let shell: HTMLDivElement | undefined = $state();

	fullscreen.widthOf = (side) => {
		if (app.panels[side].length === 0) return null;
		return side === 'left'
			? shownWidth(app.leftWidth, LEFT_LIMITS, window.innerWidth)
			: shownWidth(app.rightWidth, RIGHT_LIMITS, window.innerWidth);
	};

	// Only a document is shown full screen: with the last one closed, or the Combine view up,
	// the window goes back to what it was.
	$effect(() => {
		if (fullscreen.on && !app.active) void fullscreen.leave().catch(() => {});
	});

	function onPointerMove(event: PointerEvent) {
		// Over a menu or a dialog, which are outside the shell, the panes stay as they are.
		if (!fullscreen.on || !(event.target instanceof Node) || !shell?.contains(event.target)) return;
		fullscreen.pointerMoved(event.clientX, event.buttons, window.innerWidth);
	}

	function onPointerDown(event: PointerEvent) {
		if (fullscreen.on && event.target instanceof Element && event.target.closest('.viewer-scroll')) fullscreen.pagePressed();
	}

	// Alt held shows the panes and the toolbar. Capturing, so that fields which keep their
	// keys from the window don't keep this one; Alt by itself must not reach the system, which
	// would open the window's menu.
	function watchAlt(event: KeyboardEvent) {
		const held = event.type === 'keydown' ? fullscreen.keyDown(event) : fullscreen.keyUp(event);
		if (held) event.preventDefault();
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

	// Files dropped on a grid of pages are for it, not for tabs: Rust opens them accordingly.
	$effect(() => {
		void app.dropGrid;
		app.syncDropTarget();
	});

	// The window's title, for the taskbar and Alt+Tab: what it shows, then the app's name.
	$effect(() => {
		const tab = app.active;
		const group = tab ? app.activeGroup : null;
		const names = group?.joined ? [...new Set(group.views.map((v) => v.name))].join(', ') : null;
		const shown = names ?? tab?.name ?? (app.combineActive ? 'Combine files' : null);
		void getCurrentWindow()
			.setTitle(shown ? `${shown} - ${APP_NAME}` : APP_NAME)
			.catch(() => {});
	});

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
		const current = app.active;
		if (!current) return;
		void current.currentPage;
		void current.zoom;
		void current.rotation;
		void current.pageTurns;
		void current.ownAdjustments;
		clearTimeout(rememberTimer);
		rememberTimer = setTimeout(() => app.rememberView(current), 1500);
	});

	onMount(() => {
		window.addEventListener('keydown', blockPageZoomKey, { capture: true });
		window.addEventListener('wheel', blockPageZoomWheel, { capture: true, passive: false });
		window.addEventListener('keydown', watchAlt, { capture: true });
		window.addEventListener('keyup', watchAlt, { capture: true });
		const pointerLeft = () => fullscreen.pointerLeft();
		document.documentElement.addEventListener('mouseleave', pointerLeft);
		void startup();
		const unlisteners = [
			onDocumentsOpened((results) => app.handleOpenResults(results)),
			// Files dropped on a grid of pages, opened: the Combine view's, or that of the
			// document whose pages are being organized (section 6.7).
			onMergeSourcesAdded((results) => {
				const grid = app.dropGrid;
				if (grid === 'organize' && app.active) insertDropped(app.active, results);
				else if (app.combine && (grid === 'combine' || !grid)) app.combine.addOpened(results);
				// The grid closed while the files were opening.
				else app.discardSources(results);
			}),
			onFileChanged((event) => app.fileChanged(event.id, event.exists)),
			// Other windows (section 8): tabs moved into this one, a file opened elsewhere that
			// is open here, a save and a change of settings made in another window.
			onDocumentsMoved(() => void app.tabsMovedIn()),
			onShowDocument((id) => app.showDocument(id)),
			onDocumentSaved((event) => app.savedElsewhere(event.id, event.result)),
			onSettingsChanged(applySettings),
			onRecentChanged(() => void app.loadRecent()),
			onTouchpadPinch(dispatchPinch),
			getCurrentWindow().onResized(() => void fullscreen.windowResized().catch(() => {})),
			getCurrentWindow().onCloseRequested(async (event) => {
				if (await app.confirmExit()) return;
				event.preventDefault();
				// For a window that asked this one to close, to quit Lectrix.
				void closeDeclined().catch(() => {});
			}),
			getCurrentWebview().onDragDropEvent((event) => {
				const drop = event.payload;
				// Over the grid of a document's pages, the gap the files would go into shows,
				// in place of the sign across the window.
				const organize = app.dropGrid === 'organize' ? app.active?.organize : null;
				if (organize) {
					app.dropTarget = false;
					const ratio = window.devicePixelRatio || 1;
					if (drop.type === 'enter' || drop.type === 'over') organize.fileOver(drop.position.x / ratio, drop.position.y / ratio);
					else if (drop.type === 'drop') organize.fileDropped();
					else organize.fileLeft();
					return;
				}
				app.dropTarget = drop.type === 'enter' || drop.type === 'over';
			})
		];
		return () => {
			window.removeEventListener('keydown', blockPageZoomKey, { capture: true });
			window.removeEventListener('wheel', blockPageZoomWheel, { capture: true });
			window.removeEventListener('keydown', watchAlt, { capture: true });
			window.removeEventListener('keyup', watchAlt, { capture: true });
			document.documentElement.removeEventListener('mouseleave', pointerLeft);
			clearTimeout(rememberTimer);
			clearTimeout(panesTimer);
			for (const u of unlisteners) void u.then((f) => f());
		};
	});
</script>

<svelte:window
	onkeydown={onKeyDown}
	oncontextmenu={onContextMenu}
	onpointermove={onPointerMove}
	onpointerdown={onPointerDown}
	onblur={() => fullscreen.blurred()}
/>

<!-- Room in the view bar's row for a closed pane's button, which floats at the window's edge
     (paneButton). A pane without panels has none; while a panel is dragged, a place to drop
     it shows there instead. -->
{#snippet paneEnd(side: PaneSide)}
	{#if !organizing() && (app.panels[side].length > 0 ? !app.isOpen(side) : draggedPanel() !== null)}
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
     units (a rem is 14 px here): the pane rows and the view bar are all 40 px tall. Full
     screen, a pane lies over the pages and has its button only while it shows. -->
{#snippet paneButton(side: PaneSide)}
	{#if app.active && !organizing() && app.panels[side].length > 0 && (!full || app.isOpen(side))}
		<div
			class={['absolute top-[4px]', full ? 'z-30' : 'z-20', side === 'left' ? 'left-[4px]' : 'right-[4px]', sliding && 'chrome-slide']}
			class:chrome-away={away}
		>
			<PaneToggle {side} />
		</div>
	{/if}
{/snippet}

<!-- The annotation toolbar docked at a side of the pages (Settings), from the view bar down.
     It goes as full screen starts, sliding away with the other bars, and comes back so. -->
{#snippet toolDock(side: 'left' | 'right')}
	{#if app.active && toolDockSide === side}
		<div
			class={[
				'tool-dock',
				side === 'left' ? 'tool-dock-left' : 'tool-dock-right',
				sliding && ['chrome-slide relative z-10', side === 'left' ? 'chrome-left' : 'chrome-right']
			]}
			class:chrome-away={away}
		>
			<AnnotationToolbar tab={app.active} dock={side} />
		</div>
	{/if}
{/snippet}

<!-- macOS: the menus in the system's menu bar, whatever shows in the window. -->
{#if PLATFORM === 'macos'}
	<NativeMenus />
{/if}

<div class="flex h-full flex-col" bind:this={shell}>
	{#if !full}
		<div class={['shrink-0', sliding && 'chrome-slide chrome-up relative z-10']} class:chrome-away={away}>
			<TitleBar />
		</div>
	{/if}
	<div class="relative flex min-h-0 flex-1">
		{#if app.combineActive && app.combine}
			<main class="flex min-w-0 flex-1 flex-col bg-canvas">
				<CombineView combine={app.combine} />
			</main>
		{:else}
			{@render paneButton('left')}
			{#if app.active && app.panels.left.length > 0}
				<SidePane
					side="left"
					label="Left pane"
					open={app.isOpen('left') && !organizing()}
					overlay={full}
					{sliding}
					{away}
					bind:width={app.leftWidth}
					limits={LEFT_LIMITS}
					animate={app.panesRestored && !fullscreen.switching}
				>
					<PanelPane side="left" tab={app.active} />
				</SidePane>
			{/if}
			<main class="flex min-w-0 flex-1 flex-col bg-canvas">
				{#if app.active}
					{#if !full}
						<div
							class={['flex shrink-0 items-stretch', sliding && 'chrome-slide chrome-up-two relative z-10']}
							class:chrome-away={away}
						>
							{@render paneEnd('left')}
							<div class="min-w-0 flex-1">
								{#key app.active.key}
									{#if app.active.organize}
										<OrganizeBar tab={app.active} organize={app.active.organize} />
									{:else}
										<ViewBar tab={app.active} />
									{/if}
								{/key}
							</div>
							{@render paneEnd('right')}
						</div>
					{/if}
					{#if app.active && app.activeGroup?.joined}
					<!-- Documents side by side: each pane has its banners and its own grid of
					     pages to organize; the side panes go only while the one in focus does. -->
					<div class="flex min-h-0 flex-1">
						{#if !organizing()}{@render toolDock('left')}{/if}
						<div class={['flex min-h-0 min-w-0 flex-1', sliding ? 'viewer-covers' : 'relative']}>
							<SplitView group={app.activeGroup} {full} {floatingTools} />
						</div>
						{#if !organizing()}{@render toolDock('right')}{/if}
					</div>
					{:else}
					<!-- (Over the pages while the bars slide, as they are.) -->
					<div class={['shrink-0', sliding && 'relative z-10']}>
						<FileBanner tab={app.active} />
						<!-- Full screen, a file open for reading only says so when something tries to
						     change it (app.refuseReadOnly), not in a bar across the pages. -->
						{#if !full}
							<PdfaBanner tab={app.active} />
						{/if}
					</div>
					{#key app.active.key}
						{#if app.active.organize}
							<!-- The pages as a grid to organize (section 6.7), in the viewer's place. -->
							<OrganizeView tab={app.active} organize={app.active.organize} />
						{:else}
						<div class="flex min-h-0 flex-1">
							{@render toolDock('left')}
							<div class={['flex min-h-0 min-w-0 flex-1 flex-col', sliding ? 'viewer-covers' : 'relative']} {@attach dropTarget}>
								<Viewer tab={app.active} />
								{#if floatingTools}
									<AnnotationToolbar tab={app.active} />
								{/if}
								<AnnotationPropertiesDialog tab={app.active} />
								<BookmarkInspector tab={app.active} />
								<DropPreview />
							</div>
							{@render toolDock('right')}
						</div>
						{/if}
					{/key}
					{/if}
				{:else}
					<StartScreen />
				{/if}
			</main>
			{#if app.active && app.panels.right.length > 0}
				<SidePane
					side="right"
					label="Right pane"
					open={app.isOpen('right') && !organizing()}
					overlay={full}
					{sliding}
					{away}
					bind:width={app.rightWidth}
					limits={RIGHT_LIMITS}
					animate={app.panesRestored && !fullscreen.switching}
				>
					<PanelPane side="right" tab={app.active} />
				</SidePane>
			{/if}
			{@render paneButton('right')}
		{/if}
	</div>
</div>

<InsertPagesDialog />
<PageLabelsDialog />
<PropertiesDialog />
<ProtectDialog />
<CompressDialog />
<PrintDialog />
<SettingsDialog />
<AboutDialog />
<WhatsNewDialog />
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
