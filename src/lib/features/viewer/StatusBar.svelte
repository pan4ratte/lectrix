<script lang="ts">
	// Status bar: page box (label or number, "iv (4 of 312)"), zoom, and save state.
	import { Minus, Plus } from '@lucide/svelte';
	import { DropdownMenu } from 'bits-ui';

	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { pageBoxText, pagePosition, resolvePageInput } from './pagebox.ts';
	import { ZOOM_PRESETS, stepZoom } from './zoom.ts';

	let { tab }: { tab: DocTab } = $props();

	let editing = $state(false);
	let input: HTMLInputElement | undefined = $state();
	let draft = $state('');
	let invalid = $state(false);

	export function focusPageBox() {
		input?.focus();
		input?.select();
	}

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
	const saveState = $derived(
		tab.saving ? 'Saving…' : tab.state.dirty ? 'Unsaved changes' : 'All changes saved'
	);
</script>

<footer
	class="flex h-8 shrink-0 items-center gap-3 border-t border-line bg-chrome px-3 text-sm text-fg-muted"
>
	<div class="flex items-center gap-2">
		<label class="sr-only" for="page-box">Go to page</label>
		<input
			bind:this={input}
			id="page-box"
			class="h-6 w-16 rounded-control border bg-surface px-1.5 text-center text-fg tabular-nums outline-none focus:border-accent"
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
		<span class="tabular-nums" aria-live="polite">
			{pagePosition(tab.currentPage, tab.pageCount, tab.displayLabels)}
		</span>
		{#if invalid}
			<span class="text-danger" role="alert">No such page</span>
		{/if}
	</div>

	<div class="ml-auto flex items-center gap-1">
		<button
			type="button"
			class="icon-button size-6"
			aria-label="Zoom out (Ctrl+-)"
			title="Zoom out (Ctrl+-)"
			onclick={() => tab.viewer?.setZoom(stepZoom(tab.zoom, -1), 'custom')}
		>
			<Minus size={14} aria-hidden="true" />
		</button>
		<DropdownMenu.Root>
			<DropdownMenu.Trigger
				class="h-6 min-w-16 rounded-control px-2 text-fg tabular-nums hover:bg-hover"
				aria-label="Zoom level {zoomText}"
			>
				{zoomText}
			</DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content class="menu-content" side="top" align="end">
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
			class="icon-button size-6"
			aria-label="Zoom in (Ctrl+=)"
			title="Zoom in (Ctrl+=)"
			onclick={() => tab.viewer?.setZoom(stepZoom(tab.zoom, 1), 'custom')}
		>
			<Plus size={14} aria-hidden="true" />
		</button>
	</div>

	<span class="min-w-32 text-right" aria-live="polite">{saveState}</span>
</footer>
