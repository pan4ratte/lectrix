<script lang="ts">
	// Page labels panel (section 6.3): the label rules as a list (one row per range, its
	// pages and labels), and an editor for the selected rule below it. While a field is
	// being edited, thumbnails, the page box and the status bar show the result (live
	// preview); Enter, leaving the field or picking a style applies it as one undo step, and
	// Escape reverts it. Rows are virtualized: some files store a rule for every page.
	import { Ellipsis, Plus, Trash, TriangleAlert } from '@lucide/svelte';
	import { DropdownMenu } from 'bits-ui';
	import { tick } from 'svelte';

	import type { LabelRule, LabelStyle } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { deleteRule, removeLabels, romanFrontMatter, setRules, startRangeAt } from './actions.ts';
	import {
		STYLES,
		editableRules,
		labelsForPages,
		pagesText,
		rangeEnds,
		rangeText,
		replaceRule,
		ruleProblem,
		sameRules
	} from './rules.ts';

	let { tab }: { tab: DocTab } = $props();

	const ROW = 28;

	let list: HTMLDivElement | undefined = $state();
	let editor: HTMLElement | undefined = $state();
	let scrollTop = $state(0);
	let viewportH = $state(0);

	const editable = $derived(tab.canEditLabels);
	const shown = $derived(editableRules(tab.labelRules, tab.pageCount));
	const rules = $derived(shown.rules);
	/** The selected rule; without a selection (or when it is gone), the current page's range. */
	const selectedIndex = $derived.by(() => {
		const selected = rules.findIndex((r) => r.startPage === tab.selectedLabelRule);
		if (selected >= 0) return selected;
		const current = tab.currentPage;
		return Math.max(0, rules.findLastIndex((r) => r.startPage <= current));
	});

	/** Editing pins the shown range, so scrolling the document doesn't swap it. */
	function pin() {
		const rule = rules[selectedIndex];
		if (rule) tab.selectedLabelRule = rule.startPage;
	}

	// ----- the selected rule's fields -----

	let draft = $state({ start: '1', style: 'decimal' as LabelStyle, prefix: '', first: '1' });

	function resetDraft(rule: LabelRule | undefined) {
		draft = rule
			? { start: String(rule.startPage + 1), style: rule.style, prefix: rule.prefix, first: String(rule.firstNumber) }
			: { start: '1', style: 'decimal', prefix: '', first: '1' };
	}

	// The fields show the stored rule again whenever it changes (an applied edit, undo,
	// another selection).
	$effect(() => resetDraft(rules[selectedIndex]));

	/** Whole numbers only ("1e3" and "0x10" are not page numbers). */
	function parseWhole(text: string): number {
		return /^\s*\d+\s*$/.test(text) ? Number(text) : Number.NaN;
	}

	const draftRule = $derived<LabelRule | null>(
		selectedIndex < 0
			? null
			: {
					startPage: parseWhole(draft.start) - 1,
					style: draft.style,
					prefix: draft.prefix,
					firstNumber: draft.style === 'none' ? (rules[selectedIndex]?.firstNumber ?? 1) : parseWhole(draft.first)
				}
	);
	const problem = $derived(draftRule ? ruleProblem(draftRule, rules, selectedIndex, tab.pageCount) : null);
	const draftRules = $derived(
		draftRule && !problem ? replaceRule(rules, selectedIndex, draftRule) : null
	);
	const changed = $derived(draftRules !== null && !sameRules(draftRules, rules));

	// Live preview while a field differs from the stored rule.
	$effect(() => {
		tab.previewLabels = changed && draftRules ? labelsForPages(draftRules, tab.pageCount) : null;
		return () => {
			tab.previewLabels = null;
		};
	});

	/** Applies the fields. On leaving a field with an invalid value, it reverts instead. */
	async function commit(how: 'enter' | 'blur' | 'pick') {
		if (selectedIndex < 0) return;
		if (problem) {
			if (how === 'blur') resetDraft(rules[selectedIndex]);
			return;
		}
		if (!changed || !draftRules || !draftRule) return;
		const selected = tab.selectedLabelRule;
		const start = draftRule.startPage;
		if ((await setRules(tab, draftRules)) && tab.selectedLabelRule === selected) {
			tab.selectedLabelRule = start;
		}
	}

	// The window's shortcuts already leave typing in fields alone and keep the ones meant
	// for fields (Ctrl+S, Ctrl+G), so only the keys handled here stop.
	function onFieldKey(event: KeyboardEvent, field?: 'start' | 'first') {
		if (event.key === 'Enter') {
			event.preventDefault();
			event.stopPropagation();
			void commit('enter');
		} else if (event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
			resetDraft(rules[selectedIndex]);
		} else if (field && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
			// Step numbers with the arrow keys; the preview follows.
			event.preventDefault();
			event.stopPropagation();
			const n = parseWhole(draft[field]);
			const next = (Number.isNaN(n) ? 1 : n) + (event.key === 'ArrowUp' ? 1 : -1);
			const max = field === 'start' ? tab.pageCount : Number.MAX_SAFE_INTEGER;
			draft[field] = String(Math.min(max, Math.max(1, next)));
		}
	}

	// ----- the list -----

	/** The rules as they would be with the fields applied, for the rows. */
	const rows = $derived(draftRules ?? rules);
	const ends = $derived(rangeEnds(rows, tab.pageCount));
	const rowLabels = $derived(tab.displayLabels ?? labelsForPages(rows, tab.pageCount));
	const selectedRow = $derived(draftRules && draftRule ? rows.indexOf(draftRule) : selectedIndex);
	const slice = $derived.by(() => {
		const first = Math.max(0, Math.floor(scrollTop / ROW) - 8);
		const last = Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW) + 8);
		return rows.slice(first, last).map((rule, i) => ({ rule, index: first + i, top: (first + i) * ROW }));
	});
	const selectedEnd = $derived(selectedRow >= 0 ? (ends[selectedRow] ?? tab.pageCount) : 0);

	function ensureVisible(index: number) {
		if (!list || index < 0) return;
		const top = index * ROW;
		if (top < list.scrollTop) list.scrollTop = top;
		else if (top + ROW > list.scrollTop + viewportH) list.scrollTop = top + ROW - viewportH;
	}

	$effect(() => {
		const index = selectedRow;
		void tick().then(() => ensureVisible(index));
	});

	/** Selects a range and shows its first page. */
	function select(index: number) {
		const rule = rules[Math.max(0, Math.min(rules.length - 1, index))];
		if (!rule) return;
		tab.selectedLabelRule = rule.startPage;
		tab.viewer?.goTo({ page: rule.startPage, offset: 0 });
	}

	function onListClick(event: MouseEvent) {
		const row = (event.target as HTMLElement).closest<HTMLElement>('[data-start]');
		if (!row) return;
		const index = rules.findIndex((r) => r.startPage === Number(row.dataset.start));
		if (index >= 0) select(index);
	}

	function onListKey(event: KeyboardEvent) {
		if (event.altKey || event.ctrlKey || event.metaKey) return;
		let handled = true;
		switch (event.key) {
			case 'ArrowDown':
				select(selectedIndex + 1);
				break;
			case 'ArrowUp':
				select(selectedIndex - 1);
				break;
			case 'Home':
				select(0);
				break;
			case 'End':
				select(rules.length - 1);
				break;
			case 'Enter':
				editor?.querySelector<HTMLElement>('input:not(:disabled), select:not(:disabled)')?.focus();
				break;
			case 'Delete':
				if (editable && selectedIndex > 0) void deleteRule(tab, rules[selectedIndex]!.startPage);
				break;
			default:
				handled = false;
		}
		if (handled) {
			event.preventDefault();
			event.stopPropagation();
		}
	}

	const fieldClass =
		'field h-8 w-full min-w-0 text-sm disabled:opacity-50';
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex h-9 shrink-0 items-center gap-1 px-2">
		<h2 class="flex-1 px-1 text-xs font-semibold tracking-wide text-fg-muted uppercase">Page labels</h2>
		<button
			type="button"
			class="icon-button"
			aria-label="New range from the current page"
			title="New range from the current page"
			disabled={!editable}
			onclick={() => void startRangeAt(tab, tab.currentPage)}
		>
			<Plus size={16} aria-hidden="true" />
		</button>
		<DropdownMenu.Root>
			<DropdownMenu.Trigger class="icon-button" aria-label="More label actions" title="More label actions">
				<Ellipsis size={16} aria-hidden="true" />
			</DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content class="menu-content" align="end" sideOffset={4}>
					<DropdownMenu.Item
						class="menu-item"
						disabled={!editable || tab.currentPage === 0}
						onSelect={() => void romanFrontMatter(tab, tab.currentPage)}
					>
						Roman front matter, then 1, 2, 3 from this page
					</DropdownMenu.Item>
					<DropdownMenu.Item
						class="menu-item"
						disabled={!editable || tab.labels === null}
						onSelect={() => void removeLabels(tab)}
					>
						Remove all labels
					</DropdownMenu.Item>
				</DropdownMenu.Content>
			</DropdownMenu.Portal>
		</DropdownMenu.Root>
	</div>

	{#if !editable}
		<p class="mx-2 mb-2 flex gap-2 rounded-control bg-danger-bg p-2 text-xs" role="note">
			<TriangleAlert size={14} class="mt-0.5 shrink-0" aria-hidden="true" />
			This document’s security settings don’t allow changing page labels.
		</p>
	{/if}
	{#if tab.labels === null}
		<p class="px-3 pb-2 text-xs text-fg-muted">
			No page labels yet: pages are numbered 1, 2, 3. Change the rule below, or start a new range at a page.
		</p>
	{/if}
	{#if shown.hidden > 0}
		<p class="px-3 pb-2 text-xs text-fg-muted" role="note">
			{shown.hidden === 1 ? '1 stored rule starts' : `${shown.hidden} stored rules start`} after the last page and
			{shown.hidden === 1 ? 'has' : 'have'} no effect. Changing the labels removes {shown.hidden === 1 ? 'it' : 'them'}.
		</p>
	{/if}

	<div
		bind:this={list}
		bind:clientHeight={viewportH}
		class="tree relative min-h-0 flex-1 overflow-auto px-1 outline-none"
		role="listbox"
		aria-label="Label ranges"
		tabindex="0"
		aria-activedescendant={selectedRow >= 0 ? `label-rule-${selectedRow}` : undefined}
		onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
		onkeydown={onListKey}
		onclick={onListClick}
	>
		<div class="relative" style:height="{rows.length * ROW}px">
			{#each slice as { rule, index, top } (index)}
				{@const end = ends[index] ?? tab.pageCount}
				<!-- Rows reuse the bookmarks tree's row style. -->
				<div
					id="label-rule-{index}"
					data-start={rule.startPage}
					class="tree-row gap-2 px-2"
					style:top="{top}px"
					role="option"
					aria-selected={index === selectedRow}
					title="Pages {pagesText(rule.startPage, end)}: {rangeText(rowLabels, rule.startPage, end)}"
				>
					<span class="w-16 shrink-0 text-xs text-fg-muted tabular-nums">{pagesText(rule.startPage, end)}</span>
					<span class="truncate">{rangeText(rowLabels, rule.startPage, end)}</span>
				</div>
			{/each}
		</div>
	</div>

	{#if selectedIndex >= 0}
		<section
			bind:this={editor}
			class="flex shrink-0 flex-col gap-2 border-t border-line p-3"
			aria-label="Selected range"
			onfocusin={pin}
		>
			<h3 class="text-xs font-semibold text-fg-muted">
				{selectedRow >= 0 && draftRule ? `Pages ${pagesText(draftRule.startPage, selectedEnd)}` : 'Range'}
			</h3>
			<div class="grid grid-cols-2 gap-2">
				<!-- Fields inherit font and color (app.css), so labels keep their own small style. -->
				<label class="flex min-w-0 flex-col gap-1">
					<span class="text-xs text-fg-muted">Starts at page</span>
					<input
						class={fieldClass}
						inputmode="numeric"
						bind:value={draft.start}
						disabled={!editable || selectedIndex === 0}
						title={selectedIndex === 0 ? 'The first range always starts at page 1.' : undefined}
						aria-invalid={problem !== null && /page/i.test(problem)}
						onkeydown={(e) => onFieldKey(e, 'start')}
						onblur={() => void commit('blur')}
					/>
				</label>
				<label class="flex min-w-0 flex-col gap-1">
					<span class="text-xs text-fg-muted">First number</span>
					<input
						class={fieldClass}
						inputmode="numeric"
						bind:value={draft.first}
						disabled={!editable || draft.style === 'none'}
						title={draft.style === 'none' ? 'This style shows the prefix only.' : undefined}
						aria-invalid={problem !== null && /number/i.test(problem)}
						onkeydown={(e) => onFieldKey(e, 'first')}
						onblur={() => void commit('blur')}
					/>
				</label>
			</div>
			<label class="flex flex-col gap-1">
				<span class="text-xs text-fg-muted">Style</span>
				<select
					class={fieldClass}
					bind:value={draft.style}
					disabled={!editable}
					onchange={() => void commit('pick')}
				>
					{#each STYLES as s (s.value)}
						<option value={s.value}>{s.name}</option>
					{/each}
				</select>
			</label>
			<label class="flex flex-col gap-1">
				<span class="text-xs text-fg-muted">Prefix</span>
				<input
					class={fieldClass}
					bind:value={draft.prefix}
					disabled={!editable}
					placeholder="None"
					onkeydown={(e) => onFieldKey(e)}
					onblur={() => void commit('blur')}
				/>
			</label>
			{#if problem}
				<p class="text-xs text-danger" role="alert">{problem}</p>
			{:else if changed}
				<p class="text-xs text-fg-muted">Previewing. Enter applies it; Esc cancels.</p>
			{/if}
			<div class="flex">
				<button
					type="button"
					class="button gap-1"
					disabled={!editable || selectedIndex === 0}
					title={selectedIndex === 0 ? 'The first range can’t be deleted; every document needs one.' : undefined}
					onclick={() => void deleteRule(tab, rules[selectedIndex]!.startPage)}
				>
					<Trash size={14} aria-hidden="true" />Delete range
				</button>
			</div>
		</section>
	{/if}
</div>
