<script lang="ts">
	// A side pane's contents (section 8): a row with its panels' tabs, leaving room at its
	// outer end for the pane's button (which floats there, PaneToggle in App), then the
	// panel shown. The tabs are icons with tooltips and accessible
	// names, so they fit the narrowest pane. A tab is dragged along its row or into the other
	// pane; from the keyboard, Ctrl+Shift+Left/Right moves it, and its context menu offers
	// the same moves. The tabs look like the annotation tools, and the mark under the panel
	// shown glides to the tab picked, as it does to a tool (toolMark.ts).
	import { ContextMenu, Tabs } from 'bits-ui';
	import { tick } from 'svelte';

	import AnnotationsPanel from '#lib/features/annotations/AnnotationsPanel.svelte';
	import { toolMark } from '#lib/features/annotations/toolMark.ts';
	import BookmarksPanel from '#lib/features/bookmarks/BookmarksPanel.svelte';
	import LabelsPanel from '#lib/features/labels/LabelsPanel.svelte';
	import Thumbnails from '#lib/features/viewer/Thumbnails.svelte';
	import type { PanelId } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { PANEL_INFO, draggedPanel, dropSlot, startPanelDrag } from './panels.svelte.ts';
	import { stepPanel, type PaneSide } from './panes.ts';

	let { side, tab }: { side: PaneSide; tab: DocTab } = $props();

	const list = $derived(app.panels[side]);
	const other: PaneSide = $derived(side === 'left' ? 'right' : 'left');
	const slot = $derived(dropSlot(side));
	const needing = $derived(tab.allAnnotations.some((a) => a.problems.length > 0));

	/** The panel whose tab was right-clicked. */
	let contextPanel = $state<PanelId | null>(null);
	const contextIndex = $derived(contextPanel ? list.indexOf(contextPanel) : -1);
	// Ctrl+Shift+Left and Right: along the row, and from the left pane's inner end into the
	// right pane and back.
	const stepBack = $derived(contextIndex < 0 || !contextPanel ? null : stepPanel(app.panels, contextPanel, -1));
	const stepOn = $derived(contextIndex < 0 || !contextPanel ? null : stepPanel(app.panels, contextPanel, 1));

	async function move(panel: PanelId, to: PaneSide, index: number) {
		app.movePanel(panel, to, index);
		await tick();
		document.querySelector<HTMLElement>(`[data-panel-tab="${panel}"]`)?.focus();
	}

	function onTabKeyDown(event: KeyboardEvent, panel: PanelId) {
		if (!event.ctrlKey || !event.shiftKey || event.altKey) return;
		if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
		// Before the tabs' own arrow keys, which then do not move the focus.
		event.preventDefault();
		const step = stepPanel(app.panels, panel, event.key === 'ArrowLeft' ? -1 : 1);
		if (step) void move(panel, step.side, step.index);
	}
</script>

<Tabs.Root
	bind:value={() => app.activePanel(side) ?? '', (value) => app.setActivePanel(side, value as PanelId)}
	class="flex min-h-0 flex-1 flex-col"
>
	<div class="flex h-[40px] shrink-0 items-center gap-1 border-b border-line px-[4px] select-none" data-panel-drop={side}>
		{#if side === 'left'}
			<span class="w-[32px] shrink-0" aria-hidden="true"></span>
		{/if}
		<ContextMenu.Root>
			<ContextMenu.Trigger>
				{#snippet child({ props })}
					<Tabs.List
						{...props}
						class="flex min-w-0 items-center"
						aria-label={side === 'left' ? 'Left pane panels' : 'Right pane panels'}
					>
						<span class="tool-group flex min-w-0 items-center gap-0.5" {@attach toolMark}>
							<span class="tool-mark" aria-hidden="true" hidden></span>
							{#each list as panel, i (panel)}
								{@const info = PANEL_INFO[panel]}
								{@const Icon = info.icon}
								{@const badge = panel === 'annotations' && needing}
								<Tabs.Trigger
									value={panel}
									class="icon-button tool-button {draggedPanel() === panel ? 'opacity-50' : ''}"
									aria-label={info.label}
									aria-describedby={badge ? 'annotations-tab-needs-repair' : undefined}
									title={info.label}
									data-panel-tab={panel}
									onpointerdown={(event: PointerEvent) => startPanelDrag(panel, event)}
									onkeydown={(event: KeyboardEvent) => onTabKeyDown(event, panel)}
									oncontextmenu={() => (contextPanel = panel)}
								>
									<Icon size={18} aria-hidden="true" />
									{#if badge}
										<span class="absolute top-1 right-1 h-2 w-2 rounded-full bg-danger" aria-hidden="true"></span>
										<span id="annotations-tab-needs-repair" class="sr-only">Some annotations need repair</span>
									{/if}
									{#if slot === i}
										<span class="panel-drop-mark -left-[3px]" aria-hidden="true"></span>
									{:else if slot === list.length && i === list.length - 1}
										<span class="panel-drop-mark -right-[3px]" aria-hidden="true"></span>
									{/if}
								</Tabs.Trigger>
							{/each}
						</span>
					</Tabs.List>
				{/snippet}
			</ContextMenu.Trigger>
			<ContextMenu.Portal>
				<ContextMenu.Content class="menu-content">
					<ContextMenu.Item
						class="menu-item"
						disabled={contextIndex < 0}
						onSelect={() => contextPanel && void move(contextPanel, other, app.panels[other].length)}
					>
						{side === 'left' ? 'Move to right pane' : 'Move to left pane'}
					</ContextMenu.Item>
					<ContextMenu.Item
						class="menu-item"
						disabled={!stepBack}
						onSelect={() => contextPanel && stepBack && void move(contextPanel, stepBack.side, stepBack.index)}
					>
						Move left<span class="menu-shortcut">Ctrl+Shift+Left</span>
					</ContextMenu.Item>
					<ContextMenu.Item
						class="menu-item"
						disabled={!stepOn}
						onSelect={() => contextPanel && stepOn && void move(contextPanel, stepOn.side, stepOn.index)}
					>
						Move right<span class="menu-shortcut">Ctrl+Shift+Right</span>
					</ContextMenu.Item>
				</ContextMenu.Content>
			</ContextMenu.Portal>
		</ContextMenu.Root>
		{#if side === 'right'}
			<span class="flex-1"></span>
			<span class="w-[32px] shrink-0" aria-hidden="true"></span>
		{/if}
	</div>
	{#each list as panel (panel)}
		<Tabs.Content value={panel} class="min-h-0 flex-1">
			{#key tab.id}
				{#if panel === 'pages'}
					<Thumbnails {tab} />
				{:else if panel === 'bookmarks'}
					<BookmarksPanel {tab} />
				{:else if panel === 'labels'}
					<LabelsPanel {tab} />
				{:else}
					<AnnotationsPanel {tab} />
				{/if}
			{/key}
		</Tabs.Content>
	{/each}
</Tabs.Root>
