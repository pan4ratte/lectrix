<script lang="ts">
	// The Combine view (section 6.4): the files on the left with the options, every page
	// of every file in one grid, and the Combine button. Pages can be reordered, turned and
	// removed; the files are only read, and the result is written to a new file.
	import { ArchiveRestore, FilePlus, Layers, Redo2, RotateCcw, RotateCw, Trash, Undo2, X } from '@lucide/svelte';

	import type { BookmarkMode, LabelMode } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import type { CombineState } from './combine.svelte.ts';
	import CombineGrid from './CombineGrid.svelte';
	import CombineProgress from './CombineProgress.svelte';

	let { combine }: { combine: CombineState } = $props();

	const BOOKMARK_OPTIONS: { value: BookmarkMode; label: string }[] = [
		{ value: 'nest', label: 'One bookmark per file, with its bookmarks inside' },
		{ value: 'flat', label: 'Each file’s bookmarks as they are' },
		{ value: 'drop', label: 'No bookmarks' }
	];
	const LABEL_OPTIONS: { value: LabelMode; label: string }[] = [
		{ value: 'keep', label: 'Each page keeps its label' },
		{ value: 'continuous', label: 'Number all pages 1, 2, 3…' },
		{ value: 'none', label: 'No page labels' }
	];

	const hasSelection = $derived(combine.selected.length > 0);

	function pagesText(count: number, total: number) {
		if (count === total) return total === 1 ? '1 page' : `${total.toLocaleString()} pages`;
		if (count === 0) return 'Left out';
		return `${count.toLocaleString()} of ${total.toLocaleString()} pages`;
	}
</script>

<div class="flex min-h-0 flex-1">
	<aside class="flex w-64 shrink-0 flex-col border-r border-line bg-chrome" aria-label="Files and options">
		<div class="flex items-center justify-between px-3 pt-3 pb-1">
			<h2 class="text-sm font-semibold">Files</h2>
			<button
				type="button"
				class="icon-button size-7"
				aria-label="Add files"
				title="Add files"
				disabled={combine.running}
				onclick={() => void combine.addFiles()}
			>
				<FilePlus size={16} aria-hidden="true" />
			</button>
		</div>
		<ul class="min-h-0 flex-1 overflow-y-auto px-2" aria-label="Files">
			{#each combine.sources as source (source.id)}
				{@const count = combine.pagesFrom(source.id)}
				<li class="group flex items-center gap-2 rounded-control py-1.5 pr-1 pl-2 hover:bg-hover">
					<span
						class="size-2.5 shrink-0 rounded-full"
						style:background="var(--lectrix-source-{source.color})"
						aria-hidden="true"
					></span>
					<span class="min-w-0 flex-1">
						<span class="block truncate" title={source.path}>{source.name}</span>
						<span class="block text-xs text-fg-muted">{pagesText(count, source.pages.length)}</span>
					</span>
					{#if count < source.pages.length}
						<button
							type="button"
							class="icon-button size-7 shrink-0"
							aria-label="Put back the pages of {source.name} that were removed"
							title="Put back removed pages"
							onclick={() => combine.restoreFile(source.id)}
						>
							<ArchiveRestore size={14} aria-hidden="true" />
						</button>
					{/if}
					{#if count > 0}
						<button
							type="button"
							class="icon-button size-7 shrink-0"
							aria-label="Leave out {source.name}"
							title="Leave out this file"
							onclick={() => combine.removeFile(source.id)}
						>
							<X size={14} aria-hidden="true" />
						</button>
					{/if}
				</li>
			{/each}
		</ul>
		<div class="flex flex-col gap-4 border-t border-line p-3">
			<fieldset class="flex flex-col gap-1.5">
				<legend class="mb-1 text-sm font-semibold">Bookmarks</legend>
				{#each BOOKMARK_OPTIONS as option (option.value)}
					<label class="flex items-start gap-2 text-sm">
						<input class="mt-0.5" type="radio" bind:group={combine.bookmarks} value={option.value} />
						{option.label}
					</label>
				{/each}
			</fieldset>
			<fieldset class="flex flex-col gap-1.5">
				<legend class="mb-1 text-sm font-semibold">Page labels</legend>
				{#each LABEL_OPTIONS as option (option.value)}
					<label class="flex items-start gap-2 text-sm">
						<input class="mt-0.5" type="radio" bind:group={combine.labels} value={option.value} />
						{option.label}
					</label>
				{/each}
			</fieldset>
		</div>
	</aside>

	<section class="flex min-w-0 flex-1 flex-col" aria-label="Combine files">
		<div class="flex h-11 shrink-0 items-center gap-1 border-b border-line bg-chrome px-2" role="toolbar" aria-label="Page tools">
			<button type="button" class="button gap-2" disabled={combine.running} onclick={() => void combine.addFiles()}>
				<FilePlus size={16} aria-hidden="true" />
				Add files…
			</button>
			<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
			<button
				type="button"
				class="icon-button"
				aria-label="Turn selected pages counter-clockwise"
				title="Turn counter-clockwise (Shift+R)"
				disabled={!hasSelection}
				onclick={() => combine.rotate(combine.selectedSet, -90)}
			>
				<RotateCcw size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Turn selected pages clockwise"
				title="Turn clockwise (R)"
				disabled={!hasSelection}
				onclick={() => combine.rotate(combine.selectedSet, 90)}
			>
				<RotateCw size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Remove selected pages"
				title="Remove (Del)"
				disabled={!hasSelection}
				onclick={() => combine.remove(combine.selectedSet)}
			>
				<Trash size={16} aria-hidden="true" />
			</button>
			<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
			<button
				type="button"
				class="icon-button"
				aria-label="Undo"
				title="Undo (Ctrl+Z)"
				disabled={!combine.canUndo}
				onclick={() => combine.undo()}
			>
				<Undo2 size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Redo"
				title="Redo (Ctrl+Y)"
				disabled={!combine.canRedo}
				onclick={() => combine.redo()}
			>
				<Redo2 size={16} aria-hidden="true" />
			</button>
			<span class="ml-auto truncate pr-2 text-sm text-fg-muted" aria-live="polite">
				{#if hasSelection}{combine.selected.length.toLocaleString()} selected ·
				{/if}{combine.summary}
			</span>
		</div>

		{#if combine.pages.length === 0}
			<div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 bg-canvas p-8 text-center">
				<Layers size={40} aria-hidden="true" class="text-fg-muted" />
				<h2 class="text-lg font-semibold">
					{combine.sources.length ? 'Every page was removed' : 'Combine PDFs into one'}
				</h2>
				<p class="max-w-md text-fg-muted">
					{combine.sources.length
						? 'Put pages back from the file list, undo (Ctrl+Z), or add more files.'
						: 'Add files, or drop them here. You can reorder, turn and remove pages before combining; the files themselves are not changed.'}
				</p>
				<button type="button" class="button button-primary gap-2" onclick={() => void combine.addFiles()}>
					<FilePlus size={16} aria-hidden="true" />
					Add files…
				</button>
			</div>
		{:else}
			<CombineGrid {combine} />
		{/if}

		<div class="flex h-14 shrink-0 items-center justify-end gap-2 border-t border-line bg-chrome px-4">
			{#if combine.combinedAs}
				<span class="mr-auto truncate text-sm text-fg-muted">Combined into {combine.combinedAs}</span>
			{/if}
			<button type="button" class="button" disabled={combine.running} onclick={() => void app.closeCombine()}>
				Close
			</button>
			<button
				type="button"
				class="button button-primary"
				disabled={combine.pages.length === 0 || combine.running}
				onclick={() => void combine.combine()}
			>
				Combine…
			</button>
		</div>
	</section>
</div>

<CombineProgress {combine} />
