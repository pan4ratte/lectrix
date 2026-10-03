<script lang="ts">
	// "Insert pages from file" (section 6.4): which pages of the chosen file, where, and
	// what happens to its bookmarks and page labels. One undoable step.
	import { Dialog } from 'bits-ui';
	import { untrack } from 'svelte';

	import { parsePageRanges } from '#lib/features/viewer/ranges.ts';
	import type { BookmarkMode, InsertLabelMode } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	const request = $derived(app.insertRequest);
	const tab = $derived(request ? (app.tabs.find((t) => t.id === request.tabId) ?? null) : null);

	let which = $state<'all' | 'range'>('all');
	let rangeText = $state('');
	let where = $state<'before' | 'after'>('after');
	let pageText = $state('1');
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
			if (current.at === 0) {
				where = 'before';
				pageText = '1';
			} else {
				where = 'after';
				pageText = String(current.at);
			}
		});
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!request || !tab || busy) return;
		const count = request.source.pages.length;
		let pages: number[] = [];
		if (which === 'range') {
			const parsed = parsePageRanges(rangeText, count);
			if (!parsed?.length) {
				error = `Enter pages between 1 and ${count}, for example 1-3, 7.`;
				return;
			}
			pages = parsed;
		}
		const n = Number(pageText);
		if (!Number.isInteger(n) || n < 1 || n > tab.pageCount) {
			error = `Enter a page between 1 and ${tab.pageCount}.`;
			return;
		}
		error = '';
		busy = true;
		await app.insertPages(pages, where === 'before' ? n - 1 : n, bookmarks, labels);
		busy = false;
	}
</script>

<Dialog.Root
	open={request !== null}
	onOpenChange={(open) => {
		if (!open && !busy) app.cancelInsert();
	}}
>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay" />
		<Dialog.Content class="dialog-content w-[440px]">
			{#if request && tab}
				<Dialog.Title class="truncate text-base font-semibold">Insert pages from {request.source.name}</Dialog.Title>
				<Dialog.Description class="mt-1 text-sm text-fg-muted">
					Into {tab.name}. Inserted pages keep their annotations, links and form fields.
				</Dialog.Description>
				<form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
					<fieldset class="flex flex-col gap-2">
						<legend class="mb-1 text-sm font-medium">Pages</legend>
						<label class="flex items-center gap-2">
							<input type="radio" bind:group={which} value="all" />
							All {request.source.pages.length.toLocaleString()}
							{request.source.pages.length === 1 ? 'page' : 'pages'}
						</label>
						<label class="flex items-center gap-2">
							<input type="radio" bind:group={which} value="range" />
							Pages
							<input
								class="h-7 flex-1 rounded-control border border-line bg-surface px-2 outline-none focus:border-accent"
								bind:value={rangeText}
								onfocus={() => (which = 'range')}
								placeholder="1-3, 7"
								aria-label="Pages to insert"
							/>
						</label>
					</fieldset>
					<fieldset class="flex flex-col gap-2">
						<legend class="mb-1 text-sm font-medium">Where</legend>
						<div class="flex items-center gap-2">
							<select
								class="h-7 rounded-control border border-line bg-surface px-1 outline-none focus:border-accent"
								bind:value={where}
								aria-label="Before or after"
							>
								<option value="before">Before page</option>
								<option value="after">After page</option>
							</select>
							<input
								class="h-7 w-20 rounded-control border border-line bg-surface px-2 outline-none focus:border-accent"
								bind:value={pageText}
								inputmode="numeric"
								aria-label="Page number"
							/>
							<span class="text-sm text-fg-muted">of {tab.pageCount.toLocaleString()}</span>
						</div>
					</fieldset>
					<fieldset class="flex flex-col gap-2">
						<legend class="mb-1 text-sm font-medium">Bookmarks</legend>
						<select
							class="h-7 rounded-control border border-line bg-surface px-1 outline-none focus:border-accent"
							bind:value={bookmarks}
							aria-label="Bookmarks of the inserted file"
						>
							<option value="nest">One bookmark for the file, with its bookmarks inside</option>
							<option value="flat">The file’s bookmarks as they are</option>
							<option value="drop">No bookmarks</option>
						</select>
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
