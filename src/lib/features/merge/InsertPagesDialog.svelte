<script lang="ts">
	// "Insert pages from file" (section 6.4): which pages of the chosen file (of several
	// files, all their pages, in the files' order), and what happens to their bookmarks and
	// page labels. Where they go is decided before it opens, in the grid of pages (section
	// 6.7), and it says where. One undoable step for each file.
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { held } from '#lib/components/held.svelte.ts';
	import { insertPlace } from '#lib/features/organize/pages.ts';
	import { parsePageRanges } from '#lib/features/viewer/ranges.ts';
	import type { BookmarkMode, InsertLabelMode } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	/** The insertion asked about: the last one, which the dialog goes on showing while it
	 * closes. `app.insertRequest` is the one that can still be answered. */
	const last = held(() => app.insertRequest);
	const request = $derived(last.current);
	const tab = $derived(request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null);
	/** The file, when one came alone: its pages can be picked. */
	const alone = $derived(request?.sources.length === 1 ? request.sources[0]! : null);

	let which = $state<'all' | 'range'>('all');
	let rangeText = $state('');
	let bookmarks = $state<BookmarkMode>('nest');
	let labels = $state<InsertLabelMode>('keep');
	let error = $state('');
	let busy = $state(false);

	// A new file: start from its suggested position.
	$effect(() => {
		const current = request;
		if (!current) return;
		untrack(() => {
			which = 'all';
			rangeText = '';
			error = '';
			busy = false;
		});
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!app.insertRequest || !request || !tab || busy) return;
		let pages: number[] = [];
		if (alone && which === 'range') {
			const count = alone.pages.length;
			const parsed = parsePageRanges(rangeText, count);
			if (!parsed?.length) {
				error = `Enter pages between 1 and ${count}, for example 1-3, 7.`;
				return;
			}
			pages = parsed;
		}
		error = '';
		busy = true;
		await app.insertPages(pages, bookmarks, labels);
		busy = false;
	}
</script>

<Dialog.Root
	open={app.insertRequest !== null}
	onOpenChange={(open) => {
		if (!open && !busy) app.cancelInsert();
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-[440px]">
			{#if request && tab}
				<Dialog.Title class="truncate text-base font-semibold">
					Insert pages from {alone ? alone.name : `${request.sources.length} files`}
				</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					Into {tab.name}, {insertPlace(request.at, tab.pageCount, tab.displayLabels)}. Inserted pages keep their
					annotations, links and form fields.
				</Dialog.Description>
				<form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
					{#if !alone}
						<fieldset class="flex flex-col gap-2">
							<legend class="mb-1 text-sm font-medium">Files</legend>
							<ol class="flex max-h-[132px] flex-col gap-1 overflow-y-auto overflow-x-hidden text-sm" aria-label="Files to insert">
								{#each request.sources as source (source.id)}
									<li class="flex items-baseline justify-between gap-3">
										<span class="truncate">{source.name}</span>
										<span class="shrink-0 text-fg-muted tabular-nums">
											{source.pages.length === 1 ? '1 page' : `${source.pages.length.toLocaleString()} pages`}
										</span>
									</li>
								{/each}
							</ol>
							<p class="text-sm text-fg-muted">Every page of each file, in this order. Undo takes them back a file at a time.</p>
						</fieldset>
					{:else}
						<fieldset class="flex flex-col gap-2">
							<legend class="mb-1 text-sm font-medium">Pages</legend>
							<label class="flex items-center gap-2">
								<input type="radio" bind:group={which} value="all" />
								All {alone.pages.length.toLocaleString()}
								{alone.pages.length === 1 ? 'page' : 'pages'}
							</label>
							<label class="flex items-center gap-2">
								<input type="radio" bind:group={which} value="range" />
								Pages
								<input
									class="field h-7 flex-1"
									bind:value={rangeText}
									onfocus={() => (which = 'range')}
									placeholder="1-3, 7"
									aria-label="Pages to insert"
								/>
							</label>
						</fieldset>
					{/if}
					<fieldset class="flex flex-col gap-2">
						<legend class="mb-1 text-sm font-medium">Bookmarks</legend>
						<Dropdown
							class="h-7 w-full"
							bind:value={bookmarks}
							label={alone ? 'Bookmarks of the inserted file' : 'Bookmarks of the inserted files'}
							options={[
								{
									value: 'nest',
									label: alone
										? 'One bookmark for the file, with its bookmarks inside'
										: 'One bookmark for each file, with its bookmarks inside'
								},
								{ value: 'flat', label: alone ? 'The file’s bookmarks as they are' : 'The files’ bookmarks as they are' },
								{ value: 'drop', label: 'No bookmarks' }
							]}
						/>
					</fieldset>
					<fieldset class="flex flex-col gap-2">
						<legend class="mb-1 text-sm font-medium">Page labels</legend>
						<label class="flex items-center gap-2">
							<input type="radio" bind:group={labels} value="keep" />
							The inserted pages keep their labels
						</label>
						<label class="flex items-center gap-2">
							<input type="radio" bind:group={labels} value="follow" />
							Number them with the pages around them
						</label>
					</fieldset>
					{#if error}
						<p class="text-sm text-danger" role="alert">{error}</p>
					{/if}
					<div class="flex justify-end gap-2">
						<button type="button" class="button" disabled={busy} onclick={() => app.cancelInsert()}>Cancel</button>
						<button type="submit" class="button button-primary" disabled={busy}>
							{busy ? 'Inserting…' : 'Insert'}
						</button>
					</div>
				</form>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
