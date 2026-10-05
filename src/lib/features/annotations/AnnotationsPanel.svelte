<script lang="ts">
	// The annotation list (section 6.5): grouped by page, filtered by type and author. Each
	// row shows the type as an icon in the annotation's colour, the author and a short date,
	// and the whole comment. Clicking one shows it; a right-click offers copying the comment
	// and deleting. Replies sit under their parent, read-only (section 5.2); a badge marks
	// annotations that need repair.
	import { MessageSquareText, TriangleAlert, Wrench } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';

	import { chain } from '#lib/components/chain.ts';
	import type { Annotation } from '#lib/ipc/index.ts';
	import { stopUnlessShortcut } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { copyComment, openInspector, remove, repair, update } from './actions.ts';
	import { typeIcon } from './icons.ts';
	import { PROBLEM_SUMMARY, capabilities, formatDate, shortDate, typeName } from './tools.ts';

	let { tab }: { tab: DocTab } = $props();

	let typeFilter = $state('all');
	let authorFilter = $state('all');

	const all = $derived(tab.allAnnotations);
	const types = $derived([...new Set(all.map((a) => a.subtype))].sort((a, b) => typeName(a).localeCompare(typeName(b))));
	const authors = $derived([...new Set(all.map((a) => a.author))].sort((a, b) => a.localeCompare(b)));
	const needing = $derived(all.filter((a) => a.problems.length > 0).length);

	/** Is `a` a reply to an annotation that is on its page (shown under it)? */
	function isReply(a: Annotation) {
		return a.replyTo !== null && tab.annotation(a.page, a.replyTo) !== null;
	}

	const shown = $derived(
		all.filter(
			(a) =>
				!isReply(a) &&
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

	const selectedKey = $derived(tab.selectedAnnotation ? `${tab.selectedAnnotation.page}:${tab.selectedAnnotation.id}` : null);
	const selected = $derived(tab.selectedAnnotationInfo);

	function show(a: Annotation) {
		tab.selectAnnotation(a.page, a.id);
		tab.viewer?.reveal(a.page, a.bounds, { smooth: app.settings?.smoothAnnotationScroll !== false });
		openInspector(false);
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

	/** A right-click selects the row it is on (without scrolling the page), so the menu acts
	 * on it; from the keyboard, it arrives on the selected row. */
	function onContextMenu(event: MouseEvent) {
		const row = (event.target as HTMLElement).closest<HTMLElement>('[data-annotation-row]');
		const [page, id] = (row?.dataset.annotationRow ?? '').split(':').map(Number);
		if (row && page !== undefined && id !== undefined && `${page}:${id}` !== selectedKey) tab.selectAnnotation(page, id);
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
			<select class="field h-7 min-w-0 flex-1 text-xs" aria-label="Show type" bind:value={typeFilter}>
				<option value="all">All types</option>
				{#each types as t (t)}
					<option value={t}>{typeName(t)}</option>
				{/each}
			</select>
			<select class="field h-7 min-w-0 flex-1 text-xs" aria-label="Show author" bind:value={authorFilter}>
				<option value="all">All authors</option>
				{#each authors as name (name)}
					<option value={name}>{name || 'Unknown'}</option>
				{/each}
			</select>
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
							<div class="px-2 pt-2 pb-1 text-xs font-semibold text-fg-muted" role="presentation">
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
											style:color={a.color ?? undefined}
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
											class="field comment-field mt-1 w-full py-1 text-sm"
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
										<ul class="mt-1 flex flex-col gap-1 border-l-2 border-line pl-2" aria-label="Replies">
											{#each replies as r (r.id)}
												<li class="text-xs">
													<span class="font-semibold">{r.author || 'Unknown'}:</span>
													<span class="break-words whitespace-pre-wrap">{r.contents}</span>
												</li>
											{/each}
										</ul>
									{/if}
								</div>
							{/each}
						{/each}
					</div>
				{/snippet}
			</ContextMenu.Trigger>
			<ContextMenu.Portal>
				<ContextMenu.Content class="menu-content">
					<ContextMenu.Item
						class="menu-item"
						disabled={!selected?.contents}
						onSelect={() => void copyComment(selected?.contents ?? '')}
					>
						Copy comment
					</ContextMenu.Item>
					<ContextMenu.Separator class="menu-separator" />
					<ContextMenu.Item
						class="menu-item"
						disabled={!selected || !capabilities(selected, tab.flags.canAnnotate).delete}
						onSelect={() => {
							if (selected) void remove(tab, selected.page, selected.id);
						}}
					>
						Delete annotation
						<span class="menu-shortcut">Del</span>
					</ContextMenu.Item>
				</ContextMenu.Content>
			</ContextMenu.Portal>
		</ContextMenu.Root>
	{/if}
</div>
