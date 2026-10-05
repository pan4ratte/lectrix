<script lang="ts">
	// View bar, docked at the top of the document (as in Acrobat): the page box (label or
	// number, "iv (4 of 312)") and the zoom out, zoom level and zoom in controls.
	import { Minus, Plus } from '@lucide/svelte';
	import { DropdownMenu } from 'bits-ui';

	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { pageBoxText, pagePosition, resolvePageInput } from './pagebox.ts';
	import { ZOOM_PRESETS } from './zoom.ts';

	let { tab }: { tab: DocTab } = $props();

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

	const zoomText = $derived(`${Math.round(tab.zoom * 100)}%`);
</script>

<div
	class="grid h-10 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-3 border-b border-line bg-surface px-3 text-sm text-fg-muted"
	role="toolbar"
	aria-label="Page and zoom"
	data-view-bar
>
	<div class="flex items-center justify-end gap-2">
		<label class="sr-only" for="page-box">Go to page</label>
		<input
			bind:this={input}
			id="page-box"
			class="field h-7 w-16 text-center tabular-nums"
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
		<!-- A fixed minimum width, so the box doesn't shift as the text changes. -->
		<span class="min-w-28">
			{#if invalid}
				<span class="text-danger" role="alert">No such page</span>
			{:else}
				<span class="tabular-nums" aria-live="polite" data-page-position>
					{pagePosition(tab.currentPage, tab.pageCount, tab.displayLabels)}
				</span>
			{/if}
		</span>
	</div>

	<span class="h-5 w-px bg-line" aria-hidden="true"></span>

	<div class="flex items-center gap-1">
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
</div>
