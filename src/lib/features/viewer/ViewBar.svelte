<script lang="ts">
	// The bar at the top of the document, as in Acrobat (section 8), centered: previous and
	// next page, the page box (label or number, then "(4 of 312)" or "of 312"), zoom out,
	// the zoom level and zoom in, the page display, then the annotation tools when Settings
	// dock them here.
	import { ChevronDown, ChevronUp, Minus, Plus } from '@lucide/svelte';
	import { DropdownMenu } from 'bits-ui';

	import AnnotationToolbar from '#lib/features/annotations/AnnotationToolbar.svelte';
	import { commands, setViewMode } from '#lib/commands.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { VIEW_MODES, rowCount, rowOf } from './layout.ts';
	import { pageBoxText, pageOf, pagePosition, resolvePageInput } from './pagebox.ts';
	import PageDisplayIcon from './PageDisplayIcon.svelte';
	import { ZOOM_PRESETS } from './zoom.ts';

	let { tab }: { tab: DocTab } = $props();

	const docked = $derived(app.settings?.toolbarStyle === 'panel');

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

<div
	class="flex min-h-[40px] shrink-0 flex-wrap items-center justify-center gap-x-2 gap-y-1 border-b border-line bg-chrome px-2 py-1 text-sm text-fg-muted"
	data-view-bar
>
	<div class="flex items-center gap-1" role="toolbar" aria-label="Page and zoom">
		<button
			type="button"
			class="icon-button size-7"
			aria-label="Previous page"
			title="Previous page"
			disabled={row <= 0}
			onclick={() => tab.viewer?.turn(-1)}
		>
			<ChevronUp size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button size-7"
			aria-label="Next page"
			title="Next page"
			disabled={row >= rows - 1}
			onclick={() => tab.viewer?.turn(1)}
		>
			<ChevronDown size={16} aria-hidden="true" />
		</button>
		<label class="sr-only" for="page-box">Go to page</label>
		<input
			bind:this={input}
			id="page-box"
			class="field ml-1 h-7 w-14 text-center tabular-nums"
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
			title="Type a page number or label and press Enter (Ctrl+G)"
			aria-invalid={invalid}
			data-page-box
		/>
		<!-- A minimum width, so the zoom controls don't shift as the text changes. -->
		<span class="min-w-16 pl-1 whitespace-nowrap">
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

		<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>

		<button
			type="button"
			class="icon-button size-7"
			aria-label="Zoom out (Ctrl+-)"
			title="Zoom out (Ctrl+-)"
			onclick={() => tab.viewer?.zoomStep(-1)}
		>
			<Minus size={16} aria-hidden="true" />
		</button>
		<DropdownMenu.Root>
			<DropdownMenu.Trigger
				class="h-7 min-w-16 rounded-control px-2 text-fg tabular-nums hover:bg-hover"
				aria-label="Zoom level {zoomText}"
			>
				{zoomText}
			</DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content class="menu-content" side="bottom" align="center">
					<DropdownMenu.Item class="menu-item" onSelect={() => tab.viewer?.fit('fitWidth')}>
						Fit width
						<span class="menu-shortcut">Ctrl+0</span>
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
			aria-label="Zoom in (Ctrl+=)"
			title="Zoom in (Ctrl+=)"
			onclick={() => tab.viewer?.zoomStep(1)}
		>
			<Plus size={16} aria-hidden="true" />
		</button>

		<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>

		<DropdownMenu.Root>
			<DropdownMenu.Trigger
				class="icon-button size-7"
				aria-label="Page display: {modeLabel}"
				title="Page display: {modeLabel}"
				data-page-display
			>
				<PageDisplayIcon mode={tab.mode} />
			</DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content class="menu-content" side="bottom" align="center">
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
	</div>

	{#if docked}
		<span class="h-6 w-px bg-line" aria-hidden="true"></span>
		<AnnotationToolbar {tab} />
	{/if}
</div>
