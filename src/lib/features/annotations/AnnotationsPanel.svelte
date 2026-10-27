<script lang="ts">
	// The annotation list (section 6.5). Rows are selected as in a desktop list: a click,
	// Ctrl+click, Shift+click and Shift with the arrow keys, Ctrl+A. Its header counts the
	// annotations, and has Select all (the rows shown), Search
	// (in comments, or with its switch in the text they mark), Sort (by page, author, date
	// created or modified, each under its own headings) and Filter, which opens the types (as
	// their icons), colours and authors (as pills) to pick from. Each
	// row shows the type as an icon in the annotation's colour, the author and a short date,
	// and the whole comment, then its replies. Clicking one shows it. The selected row stands
	// in for the annotation's comment panel and bar, which the page doesn't show meanwhile: it
	// has their controls (AnnotationControls) above the comment's field, comes into view when
	// the annotation is selected on the page, and takes the cursor in its comment when the
	// annotation is double-clicked there or was just made with its comment open. A right-click
	// offers replying, copying the comment and deleting, and on a reply editing, copying and
	// deleting it (ADR 0012). A badge marks annotations that need repair.
	//
	// What the search and the filters leave out is not drawn on the page either while the list
	// is on screen (`hiddenByList`). Nothing vanishes under the user's hands: the selected
	// annotation keeps its row though a change made it stop matching, until the selection, the
	// search or the filters change; and an annotation that has just appeared (made, or brought
	// back by undo) clears a search and filters that would leave it out.
	import {
		ArrowDownUp,
		FunnelX,
		ListChecks,
		ListFilter,
		LockOpen,
		MessageSquareText,
		Search,
		SearchX,
		TextQuote,
		TriangleAlert,
		Wrench,
		X
	} from '@lucide/svelte';
	import { ContextMenu, DropdownMenu } from 'bits-ui';
	import { tick, untrack } from 'svelte';
	import { MediaQuery, SvelteMap } from 'svelte/reactivity';
	import { slide } from 'svelte/transition';

	import type { CommandName } from '#lib/commands.ts';
	import { chain } from '#lib/components/chain.ts';
	import PanelEmpty from '#lib/components/PanelEmpty.svelte';
	import { motionMs } from '#lib/components/panes.ts';
	import type { Annotation, AnnotationSort } from '#lib/ipc/index.ts';
	import { unlockDocument } from '#lib/features/unprotect/actions.ts';
	import { pageName } from '#lib/features/viewer/pagebox.ts';
	import { displayShortcut, stopUnlessShortcut } from '#lib/shortcuts.ts';
	import { app, type AnnotationRows } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';
	import { legibleOnPane } from '#lib/theme.ts';

	import {
		addReply,
		copyComment,
		copyMarkedText,
		keepsComment,
		listShowsSelected,
		openInspector,
		remove,
		removeMany,
		repair,
		undoMadeForComment,
		update
	} from './actions.ts';
	import AnnotationControls from './AnnotationControls.svelte';
	import { typeIcon } from './icons.ts';
	import {
		colorKey,
		emptyFilter,
		filterCount,
		groupAnnotations,
		listRange,
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
	import { ownSort as ownSortOf, pickSort, shownSort } from './sort.ts';
	import { tools } from './state.svelte.ts';
	import { PROBLEM_SUMMARY, capabilities, formatDate, shortDate, typeName } from './tools.ts';

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
		return tools.presets.find((c) => c.value === hex)?.name ?? hex.toUpperCase();
	}

	// ----- search -----

	let searchOpen = $state(false);
	/** What the search field holds. */
	let query = $state('');
	/** How long typing has to pause before the search is run, in ms. */
	const SEARCH_DELAY_MS = 200;
	/**
	 * The text searched for: what the field holds once typing has paused. Each search works
	 * out the rows again and has the pages on screen drawn again without what it leaves out,
	 * which is not worth doing for every key. An emptied field applies at once.
	 */
	let searched = $state('');
	$effect(() => {
		const text = query;
		if (!text.trim()) {
			searched = '';
			return;
		}
		const timer = setTimeout(() => (searched = text), SEARCH_DELAY_MS);
		return () => clearTimeout(timer);
	});
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
		if (!searchOpen || !searched.trim()) return true;
		if (searchMarked) return marksText(a) && matchesSearch(marked.get(markedKey(a)) ?? '', searched);
		return matchesSearch(a.contents, searched) || tab.repliesTo(a.page, a.id).some((r) => matchesSearch(r.contents, searched));
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

	// The sort Settings keep, or this document's own (the panel is made again for each
	// document).
	let ownSort = $state<AnnotationSort>(untrack(() => ownSortOf(tab)));
	const sorting = $derived(shownSort(app.settings, ownSort));
	const sort = $derived(sorting.sort);
	const order = $derived(sorting.order);
	function pick(choice: AnnotationSort) {
		ownSort = choice;
		void pickSort(tab, choice);
	}
	const orders = $derived(orderLabels(sort));
	const sortLabel = $derived(`Sort by ${(SORTS.find((s) => s.id === sort)?.label ?? '').toLowerCase()}, ${orders[order]}`);

	/** The search field and the filters slide open and shut (in under 150 ms; not at all
	 * with reduced motion). */
	const reveal = () => ({ duration: motionMs(140) });

	/** The selected row's controls slide open, and the last selected row's shut (`holdRow`
	 * keeps the selected row's comment still meanwhile). */
	const rowControls = (node: Element) => slide(node, reveal());
	/** The selected row's comment field stands in for its text, at its size; empty, it slides
	 * open and shut with the controls (what is in it then decides, as it leaves). */
	const rowField = (node: HTMLTextAreaElement) => (node.value ? { duration: 0 } : slide(node, reveal()));

	/** Whether the search and the filters let `a` through. */
	const matches = (a: Annotation) => matchesFilter(a, filter) && found(a);
	/** The rows: what matches, and the selected annotations, which stay while they are
	 * selected (a colour or a comment may just have been changed so that one no longer
	 * matches). */
	const shown = $derived(listed.filter((a) => matches(a) || tab.isAnnotationSelected(a.page, a.id)));

	// Another search or other filters deselect an annotation they leave out: it goes from the
	// list and from the page.
	$effect(() => {
		void [filter, searched, searchMarked, searchOpen];
		untrack(() => {
			const kept = tab.selectedAnnotations.filter((sel) => {
				const a = tab.annotation(sel.page, sel.id);
				return a !== null && (tab.isThreadReply(a) || matches(a));
			});
			if (kept.length !== tab.selectedAnnotations.length) tab.selectAnnotations(kept);
		});
	});

	// An annotation that has just appeared is never hidden: a search and filters that would
	// leave it out are cleared.
	let known: Set<string> | null = null;
	$effect(() => {
		const before = known;
		known = new Set(listed.map((a) => `${a.page}:${a.id}`));
		if (!before) return;
		untrack(() => {
			if (!listed.some((a) => !before.has(`${a.page}:${a.id}`) && !matches(a))) return;
			filter = emptyFilter();
			query = '';
			searched = '';
		});
	});
	const groups = $derived(
		groupAnnotations(
			sortAnnotations(shown, sort, order),
			sort,
			(page) => `Page ${pageName(page, tab.displayLabels)}`,
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
	/** Several annotations are selected together: their rows show selected, without controls. */
	const grouped = $derived(tab.selectedGroup.length > 1);

	/** The id of the row of annotation `key` ("page:id"). */
	const rowId = (key: string) => `annotation-row-${tab.id}-${key.replace(':', '-')}`;

	// While the list is on screen, the page shows neither comment panel nor bar for an
	// annotation that has a row here (`listShowsSelected`), and doesn't draw the annotations
	// that have none (`hiddenByList`).
	const shownKeys = $derived(new Set(shown.map((a) => `${a.page}:${a.id}`)));
	$effect(() => {
		const rows: AnnotationRows = {
			tab,
			get filtering() {
				return shown.length !== listed.length;
			},
			has: (page, id) => shownKeys.has(`${page}:${id}`)
		};
		app.annotationRows = rows;
		return () => {
			if (app.annotationRows === rows) app.annotationRows = null;
		};
	});

	// The selected annotation's row comes into view, wherever it was selected: the row has
	// its comment and its controls. A row selected on screen keeps its comment where it was
	// while its controls slide open above it (and the last selected row's shut): the list
	// scrolls with them, as far as it can, so they rise out from under the comment. Once
	// they are open, the comment is brought into view if it isn't.
	let held: { key: string; y: number } | null = null;
	let holding = 0;

	/** Where what follows a row's first line or its controls starts (its comment, replies or
	 * reply), or else the row's bottom: the point that stays put. */
	function holdPoint(row: HTMLElement): { at: number; follower: HTMLElement | null } {
		let next = row.querySelector('.annotation-meta')?.nextElementSibling;
		if (next?.hasAttribute('data-row-controls')) next = next.nextElementSibling;
		const follower = next instanceof HTMLElement ? next : null;
		return { at: (follower ?? row).getBoundingClientRect()[follower ? 'top' : 'bottom'], follower };
	}

	$effect.pre(() => {
		const key = selectedKey;
		untrack(() => {
			held = null;
			const row = key ? document.getElementById(rowId(key)) : null;
			const list = row?.closest<HTMLElement>('.annotation-list');
			if (!key || !row || !list) return;
			const box = list.getBoundingClientRect();
			const y = holdPoint(row).at - box.top;
			if (y >= 0 && y <= box.height) held = { key, y };
		});
	});

	$effect(() => {
		const key = selectedKey;
		if (!key) return;
		const hold = untrack(() => held);
		const row = document.getElementById(rowId(key));
		const list = row?.closest<HTMLElement>('.annotation-list');
		if (!row || !list) return;
		if (hold?.key !== key) {
			row.scrollIntoView({ block: 'nearest' });
			return;
		}
		const until = performance.now() + motionMs(140) + 50;
		const keep = () => {
			list.scrollTop += holdPoint(row).at - list.getBoundingClientRect().top - hold.y;
			if (performance.now() < until) {
				holding = requestAnimationFrame(keep);
				return;
			}
			holding = 0;
			(holdPoint(row).follower ?? row).scrollIntoView({ block: 'nearest' });
		};
		keep();
		return () => {
			cancelAnimationFrame(holding);
			holding = 0;
		};
	});

	// A double-click on the annotation in the page, or an annotation just made with its
	// comment open, while the list shows it: its row comes into view with the cursor at the
	// end of its comment (the row itself when the comment can't change). Hidden by a filter,
	// it opens in the comment panel instead.
	$effect(() => {
		const key = selectedKey;
		if (!app.focusListComment || !key) return;
		const frame = requestAnimationFrame(() => {
			app.focusListComment = false;
			const row = document.getElementById(rowId(key));
			if (!row) {
				if (!listShowsSelected(tab)) openInspector(tab, true);
				return;
			}
			if (!holding) row.scrollIntoView({ block: 'nearest' });
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

	const keyOf = (a: { page: number; id: number }) => `${a.page}:${a.id}`;

	function show(a: Annotation) {
		tab.selectAnnotation(a.page, a.id);
		tab.viewer?.reveal(a.page, a.bounds, { smooth: app.settings?.smoothAnnotationScroll !== false });
		openInspector(tab, false);
	}

	// ----- selecting several -----

	/** The row the keyboard is on ("page:id"), and the one a Shift+click or Shift with the
	 * arrow keys selects from: the last row clicked or moved to without Shift. */
	let cursor = $state<string | null>(null);
	let anchor: string | null = null;
	/** The selection the list made last: one made elsewhere (on the page) moves the cursor and
	 * the anchor to the annotation it selected. */
	let madeHere: readonly unknown[] | null = null;
	$effect(() => {
		const sel = tab.selectedAnnotations;
		const key = selectedKey;
		untrack(() => {
			if (sel === madeHere || !key) return;
			cursor = key;
			anchor = key;
		});
	});

	const orderedKeys = $derived(ordered.map(keyOf));

	/** Selects the rows `keys` together; one alone is shown as a click shows it. */
	function selectKeys(keys: readonly string[]) {
		const picked = keys.flatMap((k) => named(k) ?? []);
		if (picked.length === 1) show(picked[0]!);
		else tab.selectAnnotations(picked);
		madeHere = tab.selectedAnnotations;
	}

	/** The rows from the anchor to `key`; with Ctrl, besides those selected already. */
	function selectRange(key: string, adding: boolean) {
		const range = listRange(orderedKeys, anchor, key);
		if (anchor === null || !orderedKeys.includes(anchor)) anchor = key;
		cursor = key;
		selectKeys(adding ? [...new Set([...tab.selectedAnnotations.map(keyOf), ...range])] : range);
	}

	const selectAllLabel = $derived(shown.length === listed.length ? 'Select all annotations' : 'Select the annotations shown');

	/** Every row the list shows is selected: the header's button shows pressed, and takes them
	 * out again. */
	const allSelected = $derived(shown.length > 0 && shown.every((a) => tab.isAnnotationSelected(a.page, a.id)));

	/** Every row the search and the filters leave in (the header's button, Ctrl+A). */
	function selectAll() {
		selectKeys(orderedKeys);
		focusList();
	}

	/** The header's button: selects every row shown, or, with all of them selected, none. */
	function toggleAll() {
		if (!allSelected) {
			selectAll();
			return;
		}
		tab.selectAnnotations([]);
		madeHere = tab.selectedAnnotations;
		focusList();
	}

	/** A click on a row shows its annotation; with Ctrl, adds it to those selected or takes
	 * it out, as on the page; with Shift, selects the rows from the last one clicked. */
	function clickRow(event: MouseEvent, a: Annotation) {
		const key = keyOf(a);
		const ctrl = event.ctrlKey || event.metaKey;
		if (event.shiftKey) {
			selectRange(key, ctrl);
			return;
		}
		cursor = key;
		anchor = key;
		if (ctrl) tab.toggleAnnotation(a.page, a.id);
		else show(a);
		madeHere = tab.selectedAnnotations;
	}

	/** Shift or Ctrl pressed on a row selects rows, not the text in them. */
	function pressRow(event: MouseEvent) {
		if (!(event.shiftKey || event.ctrlKey || event.metaKey) || (event.target as HTMLElement).closest('textarea, input, button')) return;
		event.preventDefault();
		focusList();
	}

	/** Where the arrow keys, Home and End go from the cursor's row. */
	function moveTo(event: KeyboardEvent): number | null {
		const last = ordered.length - 1;
		const at = orderedKeys.indexOf(cursor ?? selectedKey ?? '');
		switch (event.key) {
			case 'ArrowDown':
				return Math.min(last, at + 1);
			case 'ArrowUp':
				return at < 0 ? 0 : Math.max(0, at - 1);
			case 'Home':
				return 0;
			case 'End':
				return last;
			default:
				return null;
		}
	}

	function onListKey(event: KeyboardEvent) {
		// A key typed in a comment or a reply is the field's.
		if ((event.target as HTMLElement).closest('textarea, input')) return;
		const ctrl = event.ctrlKey || event.metaKey;
		if ((event.key.toLowerCase() === 'a' || event.code === 'KeyA') && ctrl && !event.altKey && !event.shiftKey) {
			event.preventDefault();
			selectAll();
			return;
		}
		if (event.key === 'Delete' && tab.selectedGroup.length > 1) {
			event.preventDefault();
			void removeMany(tab, tab.selectedAnnotations);
			return;
		}
		const to = moveTo(event);
		if (to !== null) {
			event.preventDefault();
			const next = ordered[to];
			if (!next) return;
			if (event.shiftKey) {
				selectRange(keyOf(next), false);
				void tick().then(() => document.getElementById(rowId(keyOf(next)))?.scrollIntoView({ block: 'nearest' }));
			} else {
				cursor = anchor = keyOf(next);
				show(next);
				madeHere = tab.selectedAnnotations;
			}
			return;
		}
		const index = ordered.findIndex((a) => keyOf(a) === selectedKey);
		if (event.key === 'Delete' && index >= 0) {
			const a = ordered[index]!;
			if (capabilities(a, tab.flags.canAnnotate).delete) {
				event.preventDefault();
				void remove(tab, a.page, a.id);
			}
		}
	}

	/** The annotation a `data-annotation-row` or `data-reply` attribute ("page:id") names. */
	function named(key: string | undefined): Annotation | null {
		const [page, id] = (key ?? '').split(':').map(Number);
		return page !== undefined && id !== undefined && !Number.isNaN(id) ? tab.annotation(page, id) : null;
	}

	/** What the context menu acts on: the reply or the row under the pointer. */
	let menuTarget = $state<Annotation | null>(null);
	/** The context menu is for the annotations selected together. */
	let menuGroup = $state(false);

	/** A right-click on a reply is for that reply; on a row, it selects the row (without
	 * scrolling the page), unless the row is one of several selected together: then it is
	 * for them all. From the keyboard, it arrives on what is selected. */
	function onContextMenu(event: MouseEvent) {
		const target = event.target as HTMLElement;
		const reply = named(target.closest<HTMLElement>('[data-reply]')?.dataset.reply);
		const row = named(target.closest<HTMLElement>('[data-annotation-row]')?.dataset.annotationRow);
		menuGroup = !reply && grouped && (!row || tab.isAnnotationSelected(row.page, row.id));
		if (menuGroup) {
			menuTarget = null;
			return;
		}
		if (row && keyOf(row) !== selectedKey) {
			tab.selectAnnotation(row.page, row.id);
			cursor = anchor = keyOf(row);
			madeHere = tab.selectedAnnotations;
		}
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

	/** Whether the search, rather than the filters alone, leaves out what there is. */
	const bySearch = $derived(searchOpen && searched.trim() !== '');

	/** The empty list's way out: what leaves everything out goes, and the list has the focus. */
	async function clearSearchAndFilters() {
		query = '';
		searched = '';
		filter = emptyFilter();
		await tick();
		focusList();
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

	/** Keys on the selected row's controls are their own, as in the comment panel: the arrows
	 * that open a dropdown don't move the selection, and Delete doesn't delete the annotation. */
	function controlsKey(event: KeyboardEvent) {
		if (event.key === 'ArrowUp' || event.key === 'ArrowDown' || event.key === 'Delete') event.stopPropagation();
	}

	/** Ctrl+Enter and Esc leave the comment, which keeps what was typed. Undo in the empty
	 * comment of an annotation just made takes the annotation back. */
	function noteKey(a: Annotation, event: KeyboardEvent) {
		if (undoMadeForComment(tab, a.page, a.id, event)) return;
		stopUnlessShortcut(event);
		if ((event.key === 'Enter' && event.ctrlKey) || event.key === 'Escape') {
			event.preventDefault();
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
				aria-label={selectAllLabel}
				data-tip={allSelected ? 'Deselect the annotations' : `${selectAllLabel} (${displayShortcut('Ctrl+A')})`}
				aria-pressed={allSelected}
				disabled={shown.length === 0}
				onclick={toggleAll}
				data-select-all
			>
				<ListChecks size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button tool-button"
				aria-label="Search annotations"
				data-tip="Search annotations"
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
					data-tip={sortLabel}
					disabled={listed.length === 0}
				>
					<ArrowDownUp size={16} aria-hidden="true" />
				</DropdownMenu.Trigger>
				<DropdownMenu.Portal>
					<DropdownMenu.Content class="menu-content" align="end" sideOffset={4}>
						<DropdownMenu.RadioGroup value={sort} onValueChange={(v) => pick({ sort: v as ListSort, order })}>
							{#each SORTS as s (s.id)}
								<DropdownMenu.RadioItem class="menu-item" value={s.id}>
									{#snippet children({ checked })}
										<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>{s.label}
									{/snippet}
								</DropdownMenu.RadioItem>
							{/each}
						</DropdownMenu.RadioGroup>
						<DropdownMenu.Separator class="menu-separator" />
						<DropdownMenu.RadioGroup value={order} onValueChange={(v) => pick({ sort, order: v as SortOrder })}>
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
				data-tip="Filter annotations"
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
							data-tip="Clear the search"
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
						data-tip="Search the text annotations mark, not their comments"
						aria-pressed={searchMarked}
						onclick={() => (searchMarked = !searchMarked)}
					>
						<TextQuote size={14} aria-hidden="true" />
					</button>
				</div>
				{#if searchMarked && readingMarked && searched.trim()}
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
							data-tip={typeName(t)}
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
								data-tip={colorName(c)}
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
				data-tip="Fix annotations other apps may not show correctly"
			>
				<Wrench size={14} aria-hidden="true" />Repair {needing === 1 ? '1 annotation' : `${needing} annotations`}…
			</button>
		{/if}
	</div>

	{#snippet key(command: CommandName)}
		{#if app.shortcut(command)}
			{' '}<kbd class="keycap">{app.shortcut(command)}</kbd>
		{/if}
	{/snippet}
	{#snippet clearAction()}
		<button type="button" class="button h-7 gap-1 text-xs" onclick={() => void clearSearchAndFilters()}>
			<X size={14} aria-hidden="true" />{bySearch && filtering ? 'Clear search and filters' : bySearch ? 'Clear search' : 'Clear filters'}
		</button>
	{/snippet}

	{#snippet unlockAction()}
		<button type="button" class="button h-7 gap-1 text-xs" onclick={() => void unlockDocument(tab)}>
			<LockOpen size={14} aria-hidden="true" />Unlock…
		</button>
	{/snippet}

	{#if all.length === 0}
		<!-- A document whose security settings refuse annotating offers to unlock it (section 6.8). -->
		<PanelEmpty
			icon={MessageSquareText}
			heading="No annotations"
			action={!tab.readOnly && !tab.flags.canAnnotate ? unlockAction : undefined}
		>
			{#if tab.readOnly}
				This document is open for reading only.
			{:else if !tab.flags.canAnnotate}
				This document’s security settings don’t allow adding annotations.
			{:else}
				Highlight text{@render key('toolHighlight')} or add a note{@render key('toolNote')} from the toolbar.
			{/if}
		</PanelEmpty>
	{:else if shown.length === 0}
		<PanelEmpty icon={bySearch ? SearchX : FunnelX} heading="No matches" action={clearAction}>
			No annotations match {bySearch && filtering ? 'the search and the filters' : bySearch ? 'the search' : 'the filters'}.
		</PanelEmpty>
	{:else}
		<ContextMenu.Root>
			<ContextMenu.Trigger>
				{#snippet child({ props })}
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<div
						{...props}
						class="annotation-list min-h-0 flex-1 overflow-y-auto overflow-x-hidden p-1"
						role="listbox"
						aria-label="Annotations"
						aria-activedescendant={cursor && shownKeys.has(cursor) ? rowId(cursor) : selectedKey ? rowId(selectedKey) : undefined}
						aria-multiselectable={tab.selectedGroup.length > 1}
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
									id={rowId(key)}
									aria-selected={isSelected || (grouped && tab.isAnnotationSelected(a.page, a.id))}
									data-annotation-row={key}
									data-cursor={grouped && key === cursor ? '' : undefined}
									onmousedown={pressRow}
									onclick={(e) => clickRow(e, a)}
								>
									<div class="annotation-meta flex min-w-0 items-center gap-2 text-xs text-fg-muted">
										<span
											class="annotation-type-icon"
											style:color={iconColor(a)}
											role="img"
											aria-label={typeName(a.subtype)}
											data-tip={typeName(a.subtype)}
										>
											<Icon size={16} aria-hidden="true" />
										</span>
										<span class="min-w-0 truncate">{a.author || 'Unknown'}</span>
										{#if a.modified !== null}
											<span class="shrink-0" data-tip={formatDate(a.modified)}>{shortDate(a.modified)}</span>
										{/if}
										{#if a.problems.length}
											<span class="repair-badge" data-tip={problemsText(a)}>
												<TriangleAlert size={12} aria-hidden="true" />Needs repair
											</span>
										{/if}
									</div>
									{#if isSelected}
										<!-- A click on the controls is theirs: it doesn't show the annotation again.
										     (Inset so the colour in its button lines up with the icon above and the
										     comment below.) They slide open as the row is selected, as the last
										     selected row's slide shut, rising out from under the comment. -->
										<!-- svelte-ignore a11y_no_static_element_interactions -->
										<div
											class="mt-1 pl-[4px]"
											onclick={(e) => e.stopPropagation()}
											onkeydown={controlsKey}
											transition:rowControls
											data-row-controls
										>
											<AnnotationControls {tab} annotation={a} onremoved={focusList} />
										</div>
									{/if}
									{#if isSelected && caps.text}
										<textarea
											transition:rowField
											class="field comment-field mt-1 w-full text-sm"
											value={a.contents}
											aria-label={a.kind === 'freeText' ? 'Text' : 'Note'}
											placeholder="Add a note"
											onclick={(e) => e.stopPropagation()}
											onkeydown={(e) => noteKey(a, e)}
											{@attach keepsComment(tab, a.page, a.id)}
										></textarea>
									{:else if a.contents}
										<p class="comment-text mt-1 text-sm break-words whitespace-pre-wrap">{a.contents}</p>
									{/if}
									{#if replies.length}
										<ul class="reply-thread" aria-label="Replies">
											{#each replies as r (r.id)}
												<li class="flex flex-col gap-0.5" data-reply={keyOf(r)}>
													<div class="flex min-w-0 items-center gap-2 text-xs text-fg-muted">
														<span class="min-w-0 truncate font-semibold">{r.author || 'Unknown'}</span>
														{#if r.modified !== null}
															<span class="shrink-0" data-tip={formatDate(r.modified)}>{shortDate(r.modified)}</span>
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
					{#if menuGroup}
						{@const group = tab.selectedGroup}
						<ContextMenu.Item
							class="menu-item"
							disabled={!group.some((a) => capabilities(a, tab.flags.canAnnotate).delete)}
							onSelect={() => void removeMany(tab, tab.selectedAnnotations)}
						>
							Delete {group.length} annotations
							<span class="menu-shortcut">Del</span>
						</ContextMenu.Item>
					{:else if menuTarget}
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
						<ContextMenu.Item class="menu-item" disabled={!marksText(t)} onSelect={() => void copyMarkedText(tab, t)}>
							Copy selection
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
