<script lang="ts">
	// The annotation list (section 6.5). Its header counts the annotations, and has Search
	// (in comments, or with its switch in the text they mark), Sort (by page, author, date
	// created or modified, each under its own headings) and Filter, which opens the types (as
	// their icons), colours and authors (as pills) to pick from. Each
	// row shows the type as an icon in the annotation's colour, the author and a short date,
	// and the whole comment, then its replies. Clicking one shows it; double-clicking an
	// annotation on the page while the list shows focuses its comment here; a right-click offers
	// replying, copying the comment and deleting, and on a reply editing, copying and
	// deleting it (ADR 0012). A badge marks annotations that need repair.
	import { ArrowDownUp, ListFilter, MessageSquareText, Search, TextQuote, TriangleAlert, Wrench, X } from '@lucide/svelte';
	import { ContextMenu, DropdownMenu } from 'bits-ui';
	import { untrack } from 'svelte';
	import { MediaQuery, SvelteMap } from 'svelte/reactivity';
	import { slide } from 'svelte/transition';

	import { chain } from '#lib/components/chain.ts';
	import { motionMs } from '#lib/components/panes.ts';
	import type { Annotation } from '#lib/ipc/index.ts';
	import { stopUnlessShortcut } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';
	import { legibleOnPane } from '#lib/theme.ts';

	import { addReply, copyComment, openInspector, remove, repair, update } from './actions.ts';
	import { typeIcon } from './icons.ts';
	import {
		colorKey,
		emptyFilter,
		filterCount,
		groupAnnotations,
		markedText,
		marksText,
		matchesFilter,
		matchesSearch,
		orderLabels,
		sortAnnotations,
		SORTS,
		type AnnotationFilter,
		type ListSort,
		type SortOrder
	} from './listing.ts';
	import { PRESET_COLORS, PROBLEM_SUMMARY, capabilities, formatDate, shortDate, typeName } from './tools.ts';

	let { tab }: { tab: DocTab } = $props();

	const all = $derived(tab.allAnnotations);
	/** The list's own rows: replies show in their thread. */
	const listed = $derived(all.filter((a) => !tab.isThreadReply(a)));
	const types = $derived([...new Set(listed.map((a) => a.subtype))].sort((a, b) => typeName(a).localeCompare(typeName(b))));
	const colors = $derived([...new Set(listed.map(colorKey).filter((c) => c !== null))].sort());
	const authors = $derived([...new Set(listed.map((a) => a.author))].sort((a, b) => a.localeCompare(b)));
	const needing = $derived(all.filter((a) => a.problems.length > 0).length);

	// ----- filter -----

	let filterOpen = $state(false);
	let filter = $state<AnnotationFilter>(emptyFilter());
	const filtering = $derived(filterCount(filter));

	function toggle(set: ReadonlySet<string>, value: string): Set<string> {
		const next = new Set(set);
		if (next.has(value)) next.delete(value);
		else next.add(value);
		return next;
	}

	function colorName(hex: string): string {
		return PRESET_COLORS.find((c) => c.value === hex)?.name ?? hex.toUpperCase();
	}

	// ----- search -----

	let searchOpen = $state(false);
	let query = $state('');
	/** Search the text the annotations mark rather than their comments. */
	let searchMarked = $state(false);
	let searchField: HTMLInputElement | undefined = $state();

	/** Marked text by annotation and quads ("page:id:quads"): the same marks on the same
	 * page mark the same text, whatever else changed. */
	const marked = new SvelteMap<string, string>();
	const markedKey = (a: Annotation) => `${a.page}:${a.id}:${a.quads.join(',')}`;
	let readingMarked = $state(false);

	// Searching the marked text reads the text of each page with text markup, once.
	$effect(() => {
		if (!searchOpen || !searchMarked) return;
		const rows = listed;
		// Filling the cache must not start this again.
		const missing = untrack(() => rows.filter((a) => marksText(a) && !marked.has(markedKey(a))));
		if (!missing.length) return;
		let cancelled = false;
		readingMarked = true;
		void (async () => {
			for (const page of new Set(missing.map((a) => a.page))) {
				const text = await tab.loadText(page);
				if (cancelled) return;
				for (const a of missing) {
					if (a.page === page) marked.set(markedKey(a), text ? markedText(text.lines, a.quads) : '');
				}
			}
			readingMarked = false;
		})();
		return () => {
			cancelled = true;
			readingMarked = false;
		};
	});

	function found(a: Annotation): boolean {
		if (!searchOpen || !query.trim()) return true;
		if (searchMarked) return marksText(a) && matchesSearch(marked.get(markedKey(a)) ?? '', query);
		return matchesSearch(a.contents, query) || tab.repliesTo(a.page, a.id).some((r) => matchesSearch(r.contents, query));
	}

	function openSearch() {
		searchOpen = !searchOpen;
		if (searchOpen) requestAnimationFrame(() => searchField?.focus());
		else query = '';
	}

	/** Esc empties the field, then closes it. */
	function searchKey(event: KeyboardEvent) {
		stopUnlessShortcut(event);
		if (event.key !== 'Escape') return;
		event.preventDefault();
		if (query) {
			query = '';
		} else {
			searchOpen = false;
			focusList();
		}
	}

	// ----- sort -----

	let sort = $state<ListSort>('page');
	let order = $state<SortOrder>('asc');
	const orders = $derived(orderLabels(sort));
	const sortLabel = $derived(`Sort by ${(SORTS.find((s) => s.id === sort)?.label ?? '').toLowerCase()}, ${orders[order]}`);

	/** The search field and the filters slide open and shut (in under 150 ms; not at all
	 * with reduced motion). */
	const reveal = () => ({ duration: motionMs(140) });

	const shown = $derived(listed.filter((a) => matchesFilter(a, filter) && found(a)));
	const groups = $derived(
		groupAnnotations(
			sortAnnotations(shown, sort, order),
			sort,
			(page) => `Page ${tab.displayLabels?.[page] ?? page + 1}`,
			(ms) => new Date(ms).toLocaleDateString(undefined, { dateStyle: 'medium' })
		)
	);
	/** The rows in the order they show. */
	const ordered = $derived(groups.flatMap((g) => g.items));
	const countText = $derived(
		shown.length === listed.length
			? `${listed.length} ${listed.length === 1 ? 'annotation' : 'annotations'}`
			: `${shown.length} of ${listed.length} annotations`
	);

	// Icons take the annotation's colour, made just dark (or light) enough to show on the
	// pane: a yellow highlight's icon on the light theme, a black one's on the dark.
	const systemDark = new MediaQuery('(prefers-color-scheme: dark)');
	const dark = $derived(
		app.settings?.appearance === 'dark' || (app.settings?.appearance !== 'light' && systemDark.current)
	);

	function iconColor(a: Annotation): string | undefined {
		return a.color && /^#[0-9a-f]{6}$/i.test(a.color) ? legibleOnPane(a.color, dark) : undefined;
	}

	const selectedKey = $derived(tab.selectedAnnotation ? `${tab.selectedAnnotation.page}:${tab.selectedAnnotation.id}` : null);
	const selected = $derived(tab.selectedAnnotationInfo);

	// A double-click on the annotation in the page while the list shows: its row comes into
	// view with the cursor at the end of its comment (the row itself when the comment can't
	// change). Hidden by a filter, it opens in the comment panel instead.
	$effect(() => {
		const key = selectedKey;
		if (!app.focusListComment || !key) return;
		const frame = requestAnimationFrame(() => {
			app.focusListComment = false;
			const row = document.getElementById(`annotation-row-${tab.id}-${key.replace(':', '-')}`);
			if (!row) {
				openInspector(tab, true);
				return;
			}
			row.scrollIntoView({ block: 'nearest' });
			const field = row.querySelector<HTMLTextAreaElement>('textarea.comment-field');
			if (field) {
				field.focus({ preventScroll: true });
				field.setSelectionRange(field.value.length, field.value.length);
			} else {
				row.closest<HTMLElement>('.annotation-list')?.focus({ preventScroll: true });
			}
		});
		return () => cancelAnimationFrame(frame);
	});

	function show(a: Annotation) {
		tab.selectAnnotation(a.page, a.id);
		tab.viewer?.reveal(a.page, a.bounds, { smooth: app.settings?.smoothAnnotationScroll !== false });
		openInspector(tab, false);
	}

	function onListKey(event: KeyboardEvent) {
		const index = ordered.findIndex((a) => `${a.page}:${a.id}` === selectedKey);
		if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
			event.preventDefault();
			const next = ordered[Math.max(0, Math.min(ordered.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)))];
			if (next) {
				show(next);
				document.querySelector(`[data-annotation-row="${next.page}:${next.id}"]`)?.scrollIntoView({ block: 'nearest' });
			}
		} else if (event.key === 'Delete' && index >= 0) {
			const a = ordered[index]!;
			if (capabilities(a, tab.flags.canAnnotate).delete) {
				event.preventDefault();
				void remove(tab, a.page, a.id);
			}
		}
	}

	const keyOf = (a: Annotation) => `${a.page}:${a.id}`;

	/** The annotation a `data-annotation-row` or `data-reply` attribute ("page:id") names. */
	function named(key: string | undefined): Annotation | null {
		const [page, id] = (key ?? '').split(':').map(Number);
		return page !== undefined && id !== undefined && !Number.isNaN(id) ? tab.annotation(page, id) : null;
	}

	/** What the context menu acts on: the reply or the row under the pointer. */
	let menuTarget = $state<Annotation | null>(null);

	/** A right-click on a reply is for that reply; on a row, it selects the row (without
	 * scrolling the page). From the keyboard, it arrives on the selected row. */
	function onContextMenu(event: MouseEvent) {
		const target = event.target as HTMLElement;
		const reply = named(target.closest<HTMLElement>('[data-reply]')?.dataset.reply);
		const row = named(target.closest<HTMLElement>('[data-annotation-row]')?.dataset.annotationRow);
		if (row && keyOf(row) !== selectedKey) tab.selectAnnotation(row.page, row.id);
		menuTarget = reply ?? row ?? selected;
	}

	// ----- replies (ADR 0012) -----

	/** The annotation a reply is being written to, and the reply being edited ("page:id"). */
	let replyingTo = $state<string | null>(null);
	let editingReply = $state<string | null>(null);

	/** Puts the cursor at the end of a field that just appeared (after the menu that opened
	 * it has handed focus back). */
	function focusEnd(node: HTMLTextAreaElement) {
		requestAnimationFrame(() => {
			node.focus();
			node.setSelectionRange(node.value.length, node.value.length);
		});
	}

	function focusList() {
		document.querySelector<HTMLElement>('.annotation-list')?.focus();
	}

	function startReply(a: Annotation) {
		editingReply = null;
		replyingTo = keyOf(a);
	}

	/** Leaving the field sends the reply; an empty one is dropped. */
	function commitReply(a: Annotation, event: Event) {
		const text = (event.currentTarget as HTMLTextAreaElement).value;
		replyingTo = null;
		if (text.trim()) void addReply(tab, a.page, a.id, text);
	}

	/** Ctrl+Enter sends, Escape drops the reply. */
	function replyKey(event: KeyboardEvent) {
		stopUnlessShortcut(event);
		const field = event.currentTarget as HTMLTextAreaElement;
		if (event.key === 'Enter' && event.ctrlKey) {
			event.preventDefault();
			field.blur();
			focusList();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			field.value = '';
			field.blur();
			focusList();
		}
	}

	/** Leaving the field keeps the edit; emptying a reply deletes it. */
	function commitReplyEdit(r: Annotation, event: Event) {
		const text = (event.currentTarget as HTMLTextAreaElement).value;
		editingReply = null;
		if (!text.trim()) void remove(tab, r.page, r.id);
		else if (text !== r.contents) void update(tab, r.page, r.id, { contents: text });
	}

	function replyEditKey(r: Annotation, event: KeyboardEvent) {
		if (event.key === 'Escape') (event.currentTarget as HTMLTextAreaElement).value = r.contents;
		replyKey(event);
	}

	function commitNote(a: Annotation, event: Event) {
		const value = (event.currentTarget as HTMLTextAreaElement).value;
		if (value !== a.contents) void update(tab, a.page, a.id, { contents: value });
	}

	function noteKey(a: Annotation, event: KeyboardEvent) {
		stopUnlessShortcut(event);
		if (event.key === 'Enter' && event.ctrlKey) {
			event.preventDefault();
			(event.currentTarget as HTMLElement).blur();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			(event.currentTarget as HTMLTextAreaElement).value = a.contents;
			(event.currentTarget as HTMLElement).blur();
		}
	}

	function problemsText(a: Annotation) {
		return 'Needs repair: ' + a.problems.map((p) => PROBLEM_SUMMARY[p].toLowerCase()).join(', ');
	}
</script>

<div class="flex h-full flex-col">
	<div class="flex shrink-0 flex-col border-b border-line px-2 py-[6px]">
		<div class="flex items-center gap-0.5">
			<p class="min-w-0 flex-1 truncate px-1 text-sm text-fg-muted" aria-live="polite">{countText}</p>
			<button
				type="button"
				class="icon-button tool-button"
				aria-label="Search annotations"
				title="Search annotations"
				aria-pressed={searchOpen}
				disabled={listed.length === 0}
				onclick={openSearch}
			>
				<Search size={16} aria-hidden="true" />
			</button>
			<DropdownMenu.Root>
				<DropdownMenu.Trigger
					class="icon-button"
					aria-label={sortLabel}
					title={sortLabel}
					disabled={listed.length === 0}
				>
					<ArrowDownUp size={16} aria-hidden="true" />
				</DropdownMenu.Trigger>
				<DropdownMenu.Portal>
					<DropdownMenu.Content class="menu-content" align="end" sideOffset={4}>
						<DropdownMenu.RadioGroup value={sort} onValueChange={(v) => (sort = v as ListSort)}>
							{#each SORTS as s (s.id)}
								<DropdownMenu.RadioItem class="menu-item" value={s.id}>
									{#snippet children({ checked })}
										<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>{s.label}
									{/snippet}
								</DropdownMenu.RadioItem>
							{/each}
						</DropdownMenu.RadioGroup>
						<DropdownMenu.Separator class="menu-separator" />
						<DropdownMenu.RadioGroup value={order} onValueChange={(v) => (order = v as SortOrder)}>
							{#each ['asc', 'desc'] as const as o (o)}
								<DropdownMenu.RadioItem class="menu-item" value={o}>
									{#snippet children({ checked })}
										<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>{orders[o]}
									{/snippet}
								</DropdownMenu.RadioItem>
							{/each}
						</DropdownMenu.RadioGroup>
					</DropdownMenu.Content>
				</DropdownMenu.Portal>
			</DropdownMenu.Root>
			<button
				type="button"
				class="icon-button tool-button relative"
				aria-label={filtering ? `Filter annotations (${filtering} picked)` : 'Filter annotations'}
				title="Filter annotations"
				aria-pressed={filterOpen}
				aria-expanded={filterOpen}
				disabled={listed.length === 0}
				onclick={() => (filterOpen = !filterOpen)}
			>
				<ListFilter size={16} aria-hidden="true" />
				{#if filtering}
					<span class="filter-dot" aria-hidden="true"></span>
				{/if}
			</button>
		</div>

		{#if searchOpen}
			<div class="flex flex-col gap-2 pt-2" transition:slide={reveal()}>
				<div class="field flex h-[34px] shrink-0 items-center gap-1 pr-[3px] pl-2">
					<Search size={14} class="shrink-0 text-fg-muted" aria-hidden="true" />
					<input
						bind:this={searchField}
						bind:value={query}
						class="search-input min-w-0 flex-1 bg-transparent text-sm"
						type="search"
						aria-label={searchMarked ? 'Search the marked text' : 'Search the comments'}
						placeholder={searchMarked ? 'Search the marked text' : 'Search the comments'}
						onkeydown={searchKey}
					/>
					{#if query}
						<button
							type="button"
							class="icon-button field-button"
							aria-label="Clear the search"
							title="Clear the search"
							onclick={() => {
								query = '';
								searchField?.focus();
							}}
						>
							<X size={14} aria-hidden="true" />
						</button>
					{/if}
					<button
						type="button"
						class="icon-button tool-button field-button"
						aria-label="Search the text annotations mark, not their comments"
						title="Search the text annotations mark, not their comments"
						aria-pressed={searchMarked}
						onclick={() => (searchMarked = !searchMarked)}
					>
						<TextQuote size={14} aria-hidden="true" />
					</button>
				</div>
				{#if searchMarked && readingMarked && query.trim()}
					<p class="px-1 text-xs text-fg-muted">Reading the marked text…</p>
				{/if}
			</div>
		{/if}

		{#if filterOpen}
			<div class="flex flex-col gap-2 pt-2" role="group" aria-label="Filters" transition:slide={reveal()}>
				<h3 class="filter-heading" id="filter-types-{tab.id}">Type</h3>
				<div class="flex flex-wrap items-center gap-0.5" role="group" aria-labelledby="filter-types-{tab.id}">
					{#each types as t (t)}
						{@const Icon = typeIcon(t)}
						<button
							type="button"
							class="icon-button tool-button"
							aria-label={typeName(t)}
							title={typeName(t)}
							aria-pressed={filter.types.has(t)}
							onclick={() => (filter = { ...filter, types: toggle(filter.types, t) })}
						>
							<Icon size={16} aria-hidden="true" />
						</button>
					{/each}
				</div>
				{#if colors.length}
					<h3 class="filter-heading" id="filter-colours-{tab.id}">Colour</h3>
					<div class="flex flex-wrap items-center gap-2 px-1" role="group" aria-labelledby="filter-colours-{tab.id}">
						{#each colors as c (c)}
							<button
								type="button"
								class="swatch"
								aria-label={colorName(c)}
								title={colorName(c)}
								aria-pressed={filter.colors.has(c)}
								style:--swatch={c}
								onclick={() => (filter = { ...filter, colors: toggle(filter.colors, c) })}
							></button>
						{/each}
					</div>
				{/if}
				<h3 class="filter-heading" id="filter-authors-{tab.id}">Author</h3>
				<div class="flex flex-wrap items-center gap-1 px-1" role="group" aria-labelledby="filter-authors-{tab.id}">
					{#each authors as name (name)}
						<button
							type="button"
							class="filter-pill"
							aria-pressed={filter.authors.has(name)}
							onclick={() => (filter = { ...filter, authors: toggle(filter.authors, name) })}
						>
							{name || 'Unknown'}
						</button>
					{/each}
				</div>
				{#if filtering}
					<button type="button" class="button h-7 gap-1 self-start text-xs" onclick={() => (filter = emptyFilter())}>
						<X size={14} aria-hidden="true" />Clear filters
					</button>
				{/if}
			</div>
		{/if}

		{#if needing > 0}
			<button
				type="button"
				class="button mt-2 h-7 gap-1 text-xs"
				disabled={!tab.flags.canAnnotate}
				onclick={() => void repair(tab)}
				title="Fix annotations other apps may not show correctly"
			>
				<Wrench size={14} aria-hidden="true" />Repair {needing === 1 ? '1 annotation' : `${needing} annotations`}…
			</button>
		{/if}
	</div>

	{#if all.length === 0}
		<p class="flex flex-col items-center gap-2 p-6 text-center text-sm text-fg-muted">
			<MessageSquareText size={24} aria-hidden="true" />
			No annotations yet. Pick a tool in the toolbar to add one.
		</p>
	{:else if shown.length === 0}
		<p class="p-4 text-sm text-fg-muted">
			{searchOpen && query.trim() ? 'No annotations match the search' : 'No annotations match the filters'}{filtering &&
			searchOpen &&
			query.trim()
				? ' and the filters'
				: ''}.
		</p>
	{:else}
		<ContextMenu.Root>
			<ContextMenu.Trigger>
				{#snippet child({ props })}
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<div
						{...props}
						class="annotation-list min-h-0 flex-1 overflow-y-auto p-1"
						role="listbox"
						aria-label="Annotations"
						aria-activedescendant={selectedKey ? `annotation-row-${tab.id}-${selectedKey.replace(':', '-')}` : undefined}
						tabindex="0"
						onkeydown={onListKey}
						oncontextmenu={chain(props, 'oncontextmenu', onContextMenu)}
					>
						{#each groups as g (g.key)}
							<div class="annotation-page" role="presentation">{g.title}</div>
							{#each g.items as a, i (`${a.page}:${a.id || `direct-${i}`}`)}
								{@const key = `${a.page}:${a.id}`}
								{@const isSelected = key === selectedKey}
								{@const caps = capabilities(a, tab.flags.canAnnotate)}
								{@const replies = tab.repliesTo(a.page, a.id)}
								{@const Icon = typeIcon(a.subtype)}
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<div
									class="annotation-row"
									role="option"
									tabindex="-1"
									id="annotation-row-{tab.id}-{key.replace(':', '-')}"
									aria-selected={isSelected}
									data-annotation-row={key}
									onclick={() => show(a)}
								>
									<div class="flex min-w-0 items-center gap-2 text-xs text-fg-muted">
										<span
											class="annotation-type-icon"
											style:color={iconColor(a)}
											role="img"
											aria-label={typeName(a.subtype)}
											title={typeName(a.subtype)}
										>
											<Icon size={16} aria-hidden="true" />
										</span>
										<span class="min-w-0 truncate">{a.author || 'Unknown'}</span>
										{#if a.modified !== null}
											<span class="shrink-0" title={formatDate(a.modified)}>{shortDate(a.modified)}</span>
										{/if}
										{#if a.problems.length}
											<span class="repair-badge" title={problemsText(a)}>
												<TriangleAlert size={12} aria-hidden="true" />Needs repair
											</span>
										{/if}
									</div>
									{#if isSelected && caps.text}
										<textarea
											class="field comment-field mt-1 w-full text-sm"
											value={a.contents}
											aria-label={a.kind === 'freeText' ? 'Text' : 'Note'}
											placeholder="Add a note"
											onclick={(e) => e.stopPropagation()}
											onkeydown={(e) => noteKey(a, e)}
											onblur={(e) => commitNote(a, e)}
										></textarea>
									{:else if a.contents}
										<p class="text-sm break-words whitespace-pre-wrap">{a.contents}</p>
									{/if}
									{#if replies.length}
										<ul class="reply-thread" aria-label="Replies">
											{#each replies as r (r.id)}
												<li class="flex flex-col gap-0.5" data-reply={keyOf(r)}>
													<div class="flex min-w-0 items-center gap-2 text-xs text-fg-muted">
														<span class="min-w-0 truncate font-semibold">{r.author || 'Unknown'}</span>
														{#if r.modified !== null}
															<span class="shrink-0" title={formatDate(r.modified)}>{shortDate(r.modified)}</span>
														{/if}
													</div>
													{#if editingReply === keyOf(r)}
														<textarea
															class="field comment-field w-full text-sm"
															value={r.contents}
															aria-label="Reply"
															use:focusEnd
															onclick={(e) => e.stopPropagation()}
															onkeydown={(e) => replyEditKey(r, e)}
															onblur={(e) => commitReplyEdit(r, e)}
														></textarea>
													{:else}
														<p class="text-sm break-words whitespace-pre-wrap">{r.contents}</p>
													{/if}
												</li>
											{/each}
										</ul>
									{/if}
									{#if replyingTo === key}
										<textarea
											class="field comment-field mt-1 w-full text-sm"
											aria-label="Reply to this annotation"
											placeholder="Write a reply (Ctrl+Enter to send)"
											use:focusEnd
											onclick={(e) => e.stopPropagation()}
											onkeydown={replyKey}
											onblur={(e) => commitReply(a, e)}
										></textarea>
									{/if}
								</div>
							{/each}
						{/each}
					</div>
				{/snippet}
			</ContextMenu.Trigger>
			<ContextMenu.Portal>
				<!-- A field the menu opened keeps the focus it takes, rather than the menu handing
				     it back to the list (whose blur would end the edit). -->
				<ContextMenu.Content
					class="menu-content"
					onCloseAutoFocus={(e) => {
						if (replyingTo !== null || editingReply !== null) e.preventDefault();
					}}
				>
					{#if menuTarget}
						{@const t = menuTarget}
						{@const caps = capabilities(t, tab.flags.canAnnotate)}
						{@const isReply = tab.isThreadReply(t)}
						<!-- On a reply, Reply adds to the same thread. -->
						{@const root = isReply && t.replyTo !== null ? tab.annotation(t.page, t.replyTo) : t}
						<ContextMenu.Item
							class="menu-item"
							disabled={!root || !capabilities(root, tab.flags.canAnnotate).text}
							onSelect={() => {
								if (root) startReply(root);
							}}
						>
							Reply
						</ContextMenu.Item>
						{#if isReply}
							<ContextMenu.Item class="menu-item" disabled={!caps.text} onSelect={() => (editingReply = keyOf(t))}>
								Edit reply
							</ContextMenu.Item>
						{/if}
						<ContextMenu.Item class="menu-item" disabled={!t.contents} onSelect={() => void copyComment(t.contents)}>
							Copy comment
						</ContextMenu.Item>
						<ContextMenu.Separator class="menu-separator" />
						<ContextMenu.Item class="menu-item" disabled={!caps.delete} onSelect={() => void remove(tab, t.page, t.id)}>
							{#if isReply}
								Delete reply
							{:else}
								Delete annotation
								<span class="menu-shortcut">Del</span>
							{/if}
						</ContextMenu.Item>
					{/if}
				</ContextMenu.Content>
			</ContextMenu.Portal>
		</ContextMenu.Root>
	{/if}
</div>
