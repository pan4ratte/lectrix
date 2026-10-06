<script lang="ts">
	// The annotation list (section 6.5): grouped by page, filtered by type and author. Each
	// row shows the type as an icon in the annotation's colour, the author and a short date,
	// and the whole comment, then its replies. Clicking one shows it; double-clicking an
	// annotation on the page while the list shows focuses its comment here; a right-click offers
	// replying, copying the comment and deleting, and on a reply editing, copying and
	// deleting it (ADR 0012). A badge marks annotations that need repair.
	import { MessageSquareText, TriangleAlert, Wrench } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';
	import { MediaQuery } from 'svelte/reactivity';

	import { chain } from '#lib/components/chain.ts';
	import Dropdown from '#lib/components/Dropdown.svelte';
	import type { Annotation } from '#lib/ipc/index.ts';
	import { stopUnlessShortcut } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';
	import { legibleOnPane } from '#lib/theme.ts';

	import { addReply, copyComment, openInspector, remove, repair, update } from './actions.ts';
	import { typeIcon } from './icons.ts';
	import { PROBLEM_SUMMARY, capabilities, formatDate, shortDate, typeName } from './tools.ts';

	let { tab }: { tab: DocTab } = $props();

	let typeFilter = $state('all');
	let authorFilter = $state('all');

	const all = $derived(tab.allAnnotations);
	const types = $derived([...new Set(all.map((a) => a.subtype))].sort((a, b) => typeName(a).localeCompare(typeName(b))));
	const authors = $derived([...new Set(all.map((a) => a.author))].sort((a, b) => a.localeCompare(b)));
	const needing = $derived(all.filter((a) => a.problems.length > 0).length);

	const shown = $derived(
		all.filter(
			(a) =>
				!tab.isThreadReply(a) &&
				(typeFilter === 'all' || a.subtype === typeFilter) &&
				(authorFilter === 'all' || a.author === authorFilter)
		)
	);
	const groups = $derived.by(() => {
		const out: { page: number; items: Annotation[] }[] = [];
		for (const a of shown) {
			const last = out.at(-1);
			if (last && last.page === a.page) last.items.push(a);
			else out.push({ page: a.page, items: [a] });
		}
		return out;
	});

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
		const index = shown.findIndex((a) => `${a.page}:${a.id}` === selectedKey);
		if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
			event.preventDefault();
			const next = shown[Math.max(0, Math.min(shown.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)))];
			if (next) {
				show(next);
				document.querySelector(`[data-annotation-row="${next.page}:${next.id}"]`)?.scrollIntoView({ block: 'nearest' });
			}
		} else if (event.key === 'Delete' && index >= 0) {
			const a = shown[index]!;
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
	<div class="flex shrink-0 flex-col gap-2 border-b border-line p-2">
		<div class="flex gap-2">
			<Dropdown
				class="h-7 min-w-0 flex-1 text-xs"
				label="Show type"
				bind:value={typeFilter}
				options={[{ value: 'all', label: 'All types' }, ...types.map((t) => ({ value: t, label: typeName(t) }))]}
			/>
			<Dropdown
				class="h-7 min-w-0 flex-1 text-xs"
				label="Show author"
				bind:value={authorFilter}
				options={[{ value: 'all', label: 'All authors' }, ...authors.map((name) => ({ value: name, label: name || 'Unknown' }))]}
			/>
		</div>
		{#if needing > 0}
			<button
				type="button"
				class="button h-7 gap-1 text-xs"
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
		<p class="p-4 text-sm text-fg-muted">No annotations match the filters.</p>
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
						{#each groups as g (g.page)}
							<div class="annotation-page" role="presentation">
								Page {tab.displayLabels?.[g.page] ?? g.page + 1}
							</div>
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
