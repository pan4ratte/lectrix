<script lang="ts">
	// The bar at the top of the document, as in Acrobat (section 8), centered: previous and
	// next page, the page box (label or number, then "(4 of 312)" or "of 312"), zoom out,
	// the zoom level and zoom in, then the annotation tools when Settings dock them here.
	import { ChevronDown, ChevronUp, Minus, Plus } from '@lucide/svelte';
	import { DropdownMenu } from 'bits-ui';

	import AnnotationToolbar from '#lib/features/annotations/AnnotationToolbar.svelte';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { pageBoxText, pageOf, pagePosition, resolvePageInput } from './pagebox.ts';
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

	/** Goes to the top of the page before or after the current one. Like scrolling, not
	 * recorded in back/forward history. */
	function turn(direction: 1 | -1) {
		const page = tab.currentPage + direction;
		if (page < 0 || page >= tab.pageCount) return;
		tab.viewer?.goTo({ page, offset: 0 }, { recordHistory: false });
	}

	const zoomText = $derived(`${Math.round(tab.zoom * 100)}%`);
</script>

<div
	class="flex min-h-[40px] shrink-0 flex-wrap items-center justify-center gap-x-2 gap-y-1 border-b border-line bg-surface px-2 py-1 text-sm text-fg-muted"
	data-view-bar
>
	<div class="flex items-center gap-1" role="toolbar" aria-label="Page and zoom">
		<button
			type="button"
			class="icon-button size-7"
			aria-label="Previous page"
			title="Previous page"
			disabled={tab.currentPage <= 0}
			onclick={() => turn(-1)}
		>
			<ChevronUp size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button size-7"
			aria-label="Next page"
			title="Next page"
			disabled={tab.currentPage >= tab.pageCount - 1}
			onclick={() => turn(1)}
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
	</div>

	{#if docked}
		<span class="h-6 w-px bg-line" aria-hidden="true"></span>
		<AnnotationToolbar {tab} />
	{/if}
</div>
