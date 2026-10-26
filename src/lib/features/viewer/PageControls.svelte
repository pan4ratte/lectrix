<script lang="ts">
	// The page and zoom controls (section 8): previous and next page, the page box (label or
	// number, then "(4 of 312)" or "of 312"), zoom out, the zoom level and zoom in, the page
	// display, the visual adjustments, and with documents side by side the lock that scrolls
	// them together (section 6.1, split view). In the bar at the top of the document; full screen,
	// in the floating toolbar (section 6.1), as a group of that toolbar: stacked when that
	// toolbar is at a side of the screen, with the position under the page box and the menus
	// opening beside the toolbar.
	import { ChevronDown, ChevronUp, Link2, Minus, Plus, Scaling } from '@lucide/svelte';
	import { DropdownMenu } from 'bits-ui';

	import { commands, setViewMode } from '#lib/commands.ts';
	import { menuOffset, type MenuSide } from '#lib/features/annotations/bars.ts';
	import { toggleLock, toggleZoomLock } from '#lib/features/split/sync.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { VIEW_MODES, rowCount, rowOf } from './layout.ts';
	import { pageBoxText, pageOf, pagePosition, resolvePageInput } from './pagebox.ts';
	import PageDisplayIcon from './PageDisplayIcon.svelte';
	import VisualAdjustments from './VisualAdjustments.svelte';
	import { ZOOM_PRESETS, parseZoomInput } from './zoom.ts';

	/** `side`: where the menus open; beside the controls when these are stacked. */
	let { tab, nested = false, side = 'bottom' }: { tab: DocTab; nested?: boolean; side?: MenuSide } = $props();

	const stacked = $derived(side !== 'bottom');
	/** The tab the document is in: several documents side by side, perhaps. */
	const group = $derived(app.groupOf(tab));
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

	// The zoom box: the zoom as a percentage, which can be typed over (Enter applies it, Esc
	// or leaving the box puts it back); its chevron opens the fits and the presets.
	let zoomInput: HTMLInputElement | undefined = $state();
	let zoomBox: HTMLElement | undefined = $state();
	let zoomEditing = $state(false);
	let zoomDraft = $state('');
	let zoomInvalid = $state(false);

	function onZoomFocus() {
		zoomEditing = true;
		zoomDraft = zoomText;
		zoomInvalid = false;
		queueMicrotask(() => zoomInput?.select());
	}

	function endZoomEdit() {
		zoomEditing = false;
		zoomInvalid = false;
	}

	function commitZoom() {
		const zoom = parseZoomInput(zoomDraft);
		if (zoom === null) {
			zoomInvalid = true;
			return;
		}
		endZoomEdit();
		tab.viewer?.setZoom(zoom, 'custom');
		tab.viewer?.focus();
	}

	/** Enter applies, Esc puts the zoom back; Up and Down step it as + and − do, and Alt+Down
	 * opens the menu, as in a combo box. */
	function onZoomKey(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			event.preventDefault();
			commitZoom();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
			endZoomEdit();
			tab.viewer?.focus();
		} else if (event.key === 'ArrowDown' && event.altKey) {
			event.preventDefault();
			zoomOpen = true;
		} else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
			event.preventDefault();
			tab.viewer?.zoomStep(event.key === 'ArrowUp' ? 1 : -1);
			// The box shows where the zoom is heading, ready to be typed over again.
			queueMicrotask(() => {
				zoomDraft = `${Math.round(tab.zoom * 100)}%`;
				zoomInvalid = false;
				queueMicrotask(() => zoomInput?.select());
			});
		}
	}
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
		class:field-invalid={invalid}
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
		<!-- The chevron sits at the box's right end; stacked, under the box. -->
		<div bind:this={zoomBox} class={['relative flex items-center', stacked ? 'w-[40px] flex-col gap-0.5' : 'w-[72px]']}>
			<label class="sr-only" for="zoom-box-{tab.id}">Zoom level</label>
			<input
				bind:this={zoomInput}
				id="zoom-box-{tab.id}"
				class={['field h-7 w-full text-center tabular-nums', stacked && 'text-xs']}
				style:padding={stacked ? '0 2px' : '0 21px 0 4px'}
				class:field-invalid={zoomInvalid}
				value={zoomEditing ? zoomDraft : zoomText}
				oninput={(e) => {
					zoomDraft = e.currentTarget.value;
					zoomInvalid = false;
				}}
				onfocus={onZoomFocus}
				onblur={endZoomEdit}
				onkeydown={onZoomKey}
				data-tip="Type a zoom and press Enter"
				aria-invalid={zoomInvalid}
				data-zoom-box
			/>
			{#if zoomInvalid}
				<span class="sr-only" role="alert">Type the zoom as a percentage, such as 150</span>
			{/if}
			<DropdownMenu.Trigger
				class={[
					'flex shrink-0 items-center justify-center rounded-control text-fg-muted hover:bg-hover hover:text-fg aria-expanded:bg-hover',
					stacked ? 'h-4 w-full' : 'absolute inset-y-[3px] right-[3px] w-4'
				]}
				aria-label="Zoom options"
				data-tip="Fit width, fit page and preset zooms"
			>
				<ChevronDown size={12} aria-hidden="true" />
			</DropdownMenu.Trigger>
		</div>
		<DropdownMenu.Portal>
			<DropdownMenu.Content
				class="menu-content"
				customAnchor={zoomBox}
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
	{#if group?.joined}
		<button
			type="button"
			class="icon-button tool-button size-7"
			aria-label="Synchronize scrolling"
			aria-pressed={group.locked}
			data-tip={app.hint(group.locked ? 'Scrolling together: click to scroll each by itself' : 'Synchronize scrolling', 'syncScrolling')}
			data-sync-scrolling
			onclick={() => toggleLock()}
		>
			<Link2 size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button tool-button size-7"
			aria-label="Synchronize zoom"
			aria-pressed={group.zoomLocked}
			data-tip={app.hint(group.zoomLocked ? 'Zooming together: click to zoom each by itself' : 'Synchronize zoom', 'syncZoom')}
			data-sync-zoom
			onclick={() => toggleZoomLock()}
		>
			<Scaling size={16} aria-hidden="true" />
		</button>
	{/if}
</div>
