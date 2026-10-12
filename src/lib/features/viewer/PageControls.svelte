<script lang="ts">
	// The page and zoom controls (section 8): previous and next page, the page box (label or
	// number, then "(4 of 312)" or "of 312"), zoom out, the zoom level and zoom in, the page
	// display, and the visual adjustments. In the bar at the top of the document; full screen,
	// in the floating toolbar (section 6.1), as a group of that toolbar: stacked when that
	// toolbar is at a side of the screen, with the position under the page box and the menus
	// opening beside the toolbar.
	import { ChevronDown, ChevronUp, Minus, Plus } from '@lucide/svelte';
	import { DropdownMenu } from 'bits-ui';

	import { commands, setViewMode } from '#lib/commands.ts';
	import { menuOffset, type MenuSide } from '#lib/features/annotations/bars.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { VIEW_MODES, rowCount, rowOf } from './layout.ts';
	import { pageBoxText, pageOf, pagePosition, resolvePageInput } from './pagebox.ts';
	import PageDisplayIcon from './PageDisplayIcon.svelte';
	import VisualAdjustments from './VisualAdjustments.svelte';
	import { ZOOM_PRESETS } from './zoom.ts';

	/** `side`: where the menus open; beside the controls when these are stacked. */
	let { tab, nested = false, side = 'bottom' }: { tab: DocTab; nested?: boolean; side?: MenuSide } = $props();

	const stacked = $derived(side !== 'bottom');
	const menuAlign = $derived(stacked ? 'start' : 'center');

	let zoomOpen = $state(false);
	let displayOpen = $state(false);

	/** Esc closes the menu only: it must not also deselect, pick the Select tool or leave
	 * full screen, where these menus are in the floating toolbar. */
	function onMenuKey(event: KeyboardEvent, close: () => void) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		close();
	}

	let editing = $state(false);
	let input: HTMLInputElement | undefined = $state();
	let draft = $state('');
	let invalid = $state(false);

	function onFocus() {
		editing = true;
		draft = pageBoxText(tab.currentPage, tab.displayLabels);
		invalid = false;
		queueMicrotask(() => input?.select());
	}

	function commit() {
		const index = resolvePageInput(draft, tab.displayLabels, tab.pageCount);
		if (index === null) {
			invalid = true;
			return;
		}
		invalid = false;
		editing = false;
		input?.blur();
		tab.viewer?.goTo({ page: index, offset: 0 });
		tab.viewer?.focus();
	}

	function onKeyDown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			event.preventDefault();
			commit();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
			editing = false;
			invalid = false;
			tab.viewer?.focus();
		}
	}

	/** The current row (a page, or two side by side) and how many there are: previous and
	 * next turn a row at a time. Like scrolling, not recorded in back/forward history. */
	const row = $derived(rowOf(tab.currentPage, tab.columns, tab.cover));
	const rows = $derived(rowCount(tab.pageCount, tab.columns, tab.cover));

	const zoomText = $derived(`${Math.round(tab.zoom * 100)}%`);
	const modeLabel = $derived(VIEW_MODES.find((m) => m.id === tab.mode)?.label ?? '');
</script>

{#snippet rule()}
	<span class={stacked ? 'my-1 h-px w-6 shrink-0 bg-line' : 'mx-1 h-5 w-px bg-line'} aria-hidden="true"></span>
{/snippet}

<div
	class={['flex items-center text-sm text-fg-muted', stacked ? 'flex-col gap-0.5' : 'gap-1 whitespace-nowrap']}
	role={nested ? 'group' : 'toolbar'}
	aria-label="Page and zoom"
>
	<button
		type="button"
		class="icon-button size-7"
		aria-label="Previous page"
		data-tip="Previous page"
		disabled={row <= 0}
		onclick={() => tab.viewer?.turn(-1)}
	>
		<ChevronUp size={16} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="icon-button size-7"
		aria-label="Next page"
		data-tip="Next page"
		disabled={row >= rows - 1}
		onclick={() => tab.viewer?.turn(1)}
	>
		<ChevronDown size={16} aria-hidden="true" />
	</button>
	<label class="sr-only" for="page-box">Go to page</label>
	<input
		bind:this={input}
		id="page-box"
		class={['field h-7 text-center tabular-nums', stacked ? 'my-0.5 w-[40px] px-0.5 text-xs' : 'ml-1 w-14']}
		class:border-line={!invalid}
		class:border-danger={invalid}
		value={editing ? draft : pageBoxText(tab.currentPage, tab.displayLabels)}
		oninput={(e) => (draft = e.currentTarget.value)}
		onfocus={onFocus}
		onblur={() => {
			editing = false;
			invalid = false;
		}}
		onkeydown={onKeyDown}
		data-tip={app.hint('Type a page number or label and press Enter', 'goToPage')}
		aria-invalid={invalid}
		data-page-box
	/>
	<!-- A minimum width, so the zoom controls don't shift as the text changes. Stacked, the
	     position is under the box, in the toolbar's width: "(4 of 312)" takes two lines. -->
	<span class={stacked ? 'max-w-[48px] text-center text-[11px] leading-tight text-balance' : 'min-w-16 pl-1 whitespace-nowrap'}>
		{#if invalid}
			<span class="text-danger" role="alert">No such page</span>
		{:else}
			<span class="tabular-nums" aria-hidden="true">
				{pageOf(tab.currentPage, tab.pageCount, tab.displayLabels)}
			</span>
		{/if}
		<!-- The whole position for screen readers: "iv (4 of 312)". -->
		<span class="sr-only" aria-live="polite" data-page-position>
			{pagePosition(tab.currentPage, tab.pageCount, tab.displayLabels)}
		</span>
	</span>

	{@render rule()}

	<button
		type="button"
		class="icon-button size-7"
		aria-label={app.hint('Zoom out', 'zoomOut')}
		data-tip={app.hint('Zoom out', 'zoomOut')}
		onclick={() => tab.viewer?.zoomStep(-1)}
	>
		<Minus size={16} aria-hidden="true" />
	</button>
	<DropdownMenu.Root bind:open={zoomOpen}>
		<DropdownMenu.Trigger
			class={[
				'h-7 rounded-control text-fg tabular-nums hover:bg-hover aria-expanded:bg-hover',
				stacked ? 'w-[40px] text-xs' : 'min-w-16 px-2'
			]}
			aria-label="Zoom level {zoomText}"
		>
			{zoomText}
		</DropdownMenu.Trigger>
		<DropdownMenu.Portal>
			<DropdownMenu.Content
				class="menu-content"
				{side}
				align={menuAlign}
				sideOffset={menuOffset(side, 0)}
				escapeKeydownBehavior="ignore"
				onkeydown={(e) => onMenuKey(e, () => (zoomOpen = false))}
			>
				<DropdownMenu.Item class="menu-item" onSelect={() => tab.viewer?.fit('fitWidth')}>
					Fit width
					<span class="menu-shortcut">{app.shortcut('fitWidth')}</span>
				</DropdownMenu.Item>
				<DropdownMenu.Item class="menu-item" onSelect={() => tab.viewer?.fit('fitPage')}>
					Fit page
				</DropdownMenu.Item>
				<DropdownMenu.Separator class="menu-separator" />
				{#each ZOOM_PRESETS as z (z)}
					<DropdownMenu.Item class="menu-item" onSelect={() => tab.viewer?.setZoom(z, 'custom')}>
						{Math.round(z * 100)}%
					</DropdownMenu.Item>
				{/each}
			</DropdownMenu.Content>
		</DropdownMenu.Portal>
	</DropdownMenu.Root>
	<button
		type="button"
		class="icon-button size-7"
		aria-label={app.hint('Zoom in', 'zoomIn')}
		data-tip={app.hint('Zoom in', 'zoomIn')}
		onclick={() => tab.viewer?.zoomStep(1)}
	>
		<Plus size={16} aria-hidden="true" />
	</button>

	{@render rule()}

	<DropdownMenu.Root bind:open={displayOpen}>
		<DropdownMenu.Trigger
			class="icon-button size-7"
			aria-label="Page display: {modeLabel}"
			data-tip="Page display: {modeLabel}"
			data-page-display
		>
			<PageDisplayIcon mode={tab.mode} />
		</DropdownMenu.Trigger>
		<DropdownMenu.Portal>
			<DropdownMenu.Content
				class="menu-content"
				{side}
				align={menuAlign}
				sideOffset={menuOffset(side, 0)}
				escapeKeydownBehavior="ignore"
				onkeydown={(e) => onMenuKey(e, () => (displayOpen = false))}
			>
				<DropdownMenu.RadioGroup
					value={tab.mode}
					onValueChange={(v) => {
						const mode = VIEW_MODES.find((m) => m.id === v);
						if (mode) setViewMode(mode.id);
					}}
				>
					{#each VIEW_MODES as m (m.id)}
						<DropdownMenu.RadioItem class="menu-item" value={m.id}>
							{#snippet children({ checked })}
								<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>
								<PageDisplayIcon mode={m.id} />
								{m.label}
							{/snippet}
						</DropdownMenu.RadioItem>
					{/each}
				</DropdownMenu.RadioGroup>
				<DropdownMenu.Separator class="menu-separator" />
				<DropdownMenu.CheckboxItem
					class="menu-item"
					disabled={tab.columns !== 2}
					checked={tab.cover}
					onCheckedChange={commands.toggleCoverPage}
				>
					{#snippet children({ checked })}
						<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>Cover page alone
					{/snippet}
				</DropdownMenu.CheckboxItem>
			</DropdownMenu.Content>
		</DropdownMenu.Portal>
	</DropdownMenu.Root>
	<VisualAdjustments {tab} {side} />
</div>
