<script lang="ts" module>
	import { SvelteMap } from 'svelte/reactivity';

	/** Where the user put each annotation's panel ("tab:page:id"), from the annotation's
	 * top-left corner, kept while the app runs. */
	const positions = new SvelteMap<string, { dx: number; dy: number }>();
	/** The size the user gave each annotation's panel, kept exactly while the app runs. */
	const sizes = new SvelteMap<string, { w: number; h: number }>();
	/** The size the user last gave a panel, kept while the app runs: other annotations' panels
	 * take its width, and its height as the most they take. */
	let lastSize = $state<{ w: number; h: number } | null>(null);
</script>

<script lang="ts">
	// The comment panel of the selected annotation (sections 6.5 and 8): a header with the
	// bar's controls (colour and opacity, stroke width or font size, type, properties, delete),
	// then note text, replies and repair. An annotation with a comment shows it when selected;
	// one without shows it when opened from its bar, a double-click or the annotation list. It
	// closes with Esc (the bar shows instead) or when nothing is selected. It sits beside the
	// annotation in the viewer's scrolled content, so it scrolls with the page, and a tail on
	// its border points at the annotation. Dragged from anywhere but its controls it moves
	// within the visible part of the view; its edges and corners resize it. The size given
	// stays on that annotation's panel; another annotation's takes its width, and its height
	// only as a limit, so a short comment gets a shorter panel. Where it was put
	// is kept for each annotation. The author and dates are in the Properties dialog.
	import { Info, Trash, TriangleAlert, Wrench } from '@lucide/svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { stopUnlessShortcut } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { openProperties, remove, repair, restyle, setPanel, update } from './actions.ts';
	import {
		BAR_GAP,
		barControls,
		clampPanel,
		placePanel,
		resizePanel,
		tailShape,
		type Area,
		type ResizeEdge
	} from './bars.ts';
	import ColorPicker from './ColorPicker.svelte';
	import { typeIcon } from './icons.ts';
	import { FONT_SIZES, MARKUP_SUBTYPES, PEN_WIDTHS, PROBLEM_TEXT, ptOptions, typeName, type MarkupKind } from './tools.ts';

	interface Props {
		tab: DocTab;
		/** The selected annotation, in the scrolled content. */
		anchor: Area;
		/** The visible part of the content. */
		view: Area;
		contentWidth: number;
	}

	let { tab, anchor, view, contentWidth }: Props = $props();

	const DEFAULT_WIDTH = 288;
	/** The panel's corner radius (--radius-panel), which the tail keeps clear of. */
	const RADIUS = 8;
	const EDGES: readonly ResizeEdge[] = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'];

	const a = $derived(tab.selectedAnnotationInfo);
	const key = $derived(a ? `${tab.id}:${a.page}:${a.id}` : null);
	const caps = $derived(a ? barControls(a, tab.flags.canAnnotate) : null);
	const replies = $derived(a ? tab.repliesTo(a.page, a.id) : []);
	const parent = $derived(a?.replyTo != null ? tab.annotation(a.page, a.replyTo) : null);

	let noteField: HTMLTextAreaElement | undefined = $state();
	let w = $state(0);
	let h = $state(0);
	/** The size given to this annotation's panel by resizing it. */
	const exact = $derived(key ? sizes.get(key) : undefined);
	/** Another panel was resized: its width, and its height as this one's limit. */
	const limit = $derived(exact ? null : lastSize);
	/** The most the content may take before it scrolls: the visible part of the view. */
	const viewMax = $derived(Math.max(120, view.y1 - view.y0 - 2 * BAR_GAP));

	// ----- where it is -----

	const saved = $derived(key ? positions.get(key) : undefined);
	const at = $derived(
		saved ? { left: anchor.x0 + saved.dx, top: anchor.y0 + saved.dy } : placePanel(anchor, { w, h }, view, contentWidth)
	);
	const tail = $derived(
		w === 0
			? null
			: tailShape(
					w,
					h,
					{ x0: anchor.x0 - at.left, y0: anchor.y0 - at.top, x1: anchor.x1 - at.left, y1: anchor.y1 - at.top },
					RADIUS
				)
	);
	const tailSides = $derived(
		tail
			? `M${tail.e1.x} ${tail.e1.y} Q${tail.c1.x} ${tail.c1.y} ${tail.tip.x} ${tail.tip.y} Q${tail.c2.x} ${tail.c2.y} ${tail.e2.x} ${tail.e2.y}`
			: ''
	);

	function putAt(left: number, top: number) {
		if (key) positions.set(key, { dx: left - anchor.x0, dy: top - anchor.y0 });
	}

	type Gesture =
		| { kind: 'move'; pointer: number; x: number; y: number; left: number; top: number }
		| { kind: 'resize'; pointer: number; x: number; y: number; start: Area; edge: ResizeEdge };
	let gesture: Gesture | null = null;

	/** Controls keep their own pointer behaviour; anywhere else drags the panel. */
	const CONTROLS = 'textarea, input, select, button, label, a, .panel-handle';

	function onPanelDown(event: PointerEvent) {
		if (event.button !== 0) return;
		const target = event.target as HTMLElement;
		if (target.closest(CONTROLS)) return;
		// Not on the content's own scrollbar.
		const scroll = target.closest<HTMLElement>('[data-panel-scroll]');
		if (scroll && target === scroll) {
			const r = scroll.getBoundingClientRect();
			if (event.clientX - r.left > scroll.clientWidth || event.clientY - r.top > scroll.clientHeight) return;
		}
		event.preventDefault();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		gesture = { kind: 'move', pointer: event.pointerId, x: event.clientX, y: event.clientY, left: at.left, top: at.top };
	}

	function onHandleDown(event: PointerEvent, edge: ResizeEdge) {
		if (event.button !== 0) return;
		event.preventDefault();
		event.stopPropagation();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		const start = { x0: at.left, y0: at.top, x1: at.left + w, y1: at.top + h };
		gesture = { kind: 'resize', pointer: event.pointerId, x: event.clientX, y: event.clientY, start, edge };
	}

	function onPointerMove(event: PointerEvent) {
		const g = gesture;
		if (!g || event.pointerId !== g.pointer) return;
		const dx = event.clientX - g.x;
		const dy = event.clientY - g.y;
		if (g.kind === 'move') {
			const to = clampPanel({ left: g.left + dx, top: g.top + dy }, { w, h }, view, contentWidth);
			putAt(to.left, to.top);
		} else {
			const r = resizePanel(g.start, g.edge, dx, dy, view, contentWidth);
			if (key) sizes.set(key, { w: r.x1 - r.x0, h: r.y1 - r.y0 });
			putAt(r.x0, r.y0);
		}
	}

	function onPointerUp(event: PointerEvent) {
		if (gesture?.pointer !== event.pointerId) return;
		if (gesture.kind === 'resize' && exact) lastSize = exact;
		gesture = null;
	}

	// ----- what it shows -----

	// Opened for its note (a double-click, a new note or markup): type right away, once
	// measured and shown (a hidden field can't take the focus).
	$effect(() => {
		const field = noteField;
		if (!app.focusNoteText || !field || w === 0) return;
		const frame = requestAnimationFrame(() => {
			field.focus({ preventScroll: true });
			if (document.activeElement === field) app.focusNoteText = false;
		});
		return () => cancelAnimationFrame(frame);
	});

	function commitText(event: Event) {
		if (!a) return;
		const value = (event.currentTarget as HTMLTextAreaElement).value;
		if (value !== a.contents) void update(tab, a.page, a.id, { contents: value });
	}

	function textKey(event: KeyboardEvent) {
		stopUnlessShortcut(event);
		if (event.key === 'Enter' && event.ctrlKey) {
			event.preventDefault();
			(event.currentTarget as HTMLElement).blur();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			(event.currentTarget as HTMLTextAreaElement).value = a?.contents ?? '';
			(event.currentTarget as HTMLElement).blur();
		}
	}

	/** Esc on the panel's other controls closes it. */
	function onPanelKey(event: KeyboardEvent) {
		if (event.key !== 'Escape' || event.defaultPrevented) return;
		event.preventDefault();
		event.stopPropagation();
		setPanel(tab, false);
		tab.viewer?.focus();
	}

	function setKind(kind: MarkupKind, subtype: string) {
		if (a && subtype !== a.subtype) void update(tab, a.page, a.id, { kind });
	}

	function del() {
		if (a) void remove(tab, a.page, a.id).then(() => tab.viewer?.focus());
	}

	/** No page menu over the panel (a text field keeps the browser's, for spelling). */
	function onContextMenu(event: MouseEvent) {
		event.stopPropagation();
		const target = event.target as HTMLElement;
		if (!target.closest('textarea, input')) event.preventDefault();
	}
</script>

{#if a && caps}
	<!-- The panel drags from anywhere but its controls: a pointer convenience, as the
	     viewer's own dragging is. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="absolute z-20 cursor-move"
		class:invisible={w === 0}
		style:left="{at.left}px"
		style:top="{at.top}px"
		style:width="{(exact ?? limit)?.w ?? DEFAULT_WIDTH}px"
		style:height={exact ? `${exact.h}px` : null}
		data-annotation-panel
		bind:offsetWidth={w}
		bind:offsetHeight={h}
		onpointerdown={onPanelDown}
		onpointermove={onPointerMove}
		onpointerup={onPointerUp}
		onpointercancel={onPointerUp}
		onkeydown={onPanelKey}
		oncontextmenu={onContextMenu}
	>
		{#if tail}
			<svg class="comment-tail" width={w} height={h} aria-hidden="true">
				<path class="comment-tail-fill" d="{tailSides} Z" />
				<path class="comment-tail-edge" d={tailSides} />
			</svg>
		{/if}
		<aside
			class="flex h-full flex-col gap-[12px] overflow-auto rounded-panel border border-line bg-surface-raised p-[12px] shadow-[0_4px_12px_var(--color-page-shadow)]"
			style:max-height={exact ? null : `${Math.min(limit?.h ?? Infinity, viewMax)}px`}
			aria-label="Annotation comment"
			data-panel-scroll
		>
			{#if !tab.flags.canAnnotate}
				<p class="text-xs text-fg-muted">This document’s security settings don’t allow changing annotations.</p>
			{:else if a.id === 0}
				<p class="text-xs text-fg-muted">This annotation is stored in a way Lectrix can show but not change.</p>
			{/if}

			{#if parent}
				<p class="text-xs text-fg-muted">
					Reply to {typeName(parent.subtype).toLowerCase()} by {parent.author || 'unknown'}. Replies are read-only.
				</p>
			{/if}

			{#if caps.restyle || caps.retype || caps.delete || !caps.text}
				<div class="flex flex-wrap items-center gap-1">
					{#if caps.restyle}
						<ColorPicker
							color={a.color}
							opacity={a.opacity}
							label={a.kind === 'freeText' ? 'Text colour' : 'Colour'}
							oncolor={(color) => void restyle(tab, a, { color })}
							onopacity={(opacity) => void restyle(tab, a, { opacity })}
						/>
					{/if}
					{#if caps.restyle && a.kind === 'ink'}
						<Dropdown
							class="toolbar-dropdown"
							label="Stroke width"
							title="Stroke width"
							value={a.width ?? 1}
							options={ptOptions([...PEN_WIDTHS, a.width ?? 1])}
							onchange={(width) => void update(tab, a.page, a.id, { width })}
						/>
					{/if}
					{#if caps.restyle && a.kind === 'freeText'}
						<Dropdown
							class="toolbar-dropdown"
							label="Font size"
							title="Font size"
							value={a.fontSize ?? 12}
							options={ptOptions([...FONT_SIZES, a.fontSize ?? 12])}
							onchange={(fontSize) => void update(tab, a.page, a.id, { fontSize })}
						/>
					{/if}
					{#if caps.retype}
						{#if caps.restyle}<span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>{/if}
						{#each MARKUP_SUBTYPES as m (m.subtype)}
							{@const Icon = typeIcon(m.subtype)}
							<button
								type="button"
								class="icon-button tool-button"
								aria-pressed={a.subtype === m.subtype}
								aria-label={typeName(m.subtype)}
								title={typeName(m.subtype)}
								onclick={() => setKind(m.kind, m.subtype)}
							>
								<Icon size={18} aria-hidden="true" />
							</button>
						{/each}
					{/if}
					<span class="ml-auto flex items-center gap-1">
						{#if !caps.text}
							<button type="button" class="icon-button" aria-label="Properties" title="Properties" onclick={openProperties}>
								<Info size={18} aria-hidden="true" />
							</button>
						{/if}
						{#if caps.delete}
							<button type="button" class="icon-button" aria-label="Delete" title="Delete (Del)" onclick={del}>
								<Trash size={18} aria-hidden="true" />
							</button>
						{/if}
					</span>
				</div>
			{/if}

			{#key key}
				<textarea
					bind:this={noteField}
					class="field panel-note resize-none {exact
						? 'min-h-[60px] flex-1'
						: limit
							? 'min-h-[60px] w-full [field-sizing:content]'
							: 'h-[140px] shrink-0'}"
					aria-label={a.kind === 'freeText' ? 'Text' : 'Note'}
					value={a.contents}
					readonly={!caps.text}
					placeholder={caps.text ? 'Add a note' : ''}
					onkeydown={textKey}
					onblur={commitText}
				></textarea>
			{/key}

			{#if replies.length}
				<div class="flex flex-col gap-1">
					<span class="text-xs text-fg-muted">Replies</span>
					{#each replies as r (r.id)}
						<div class="rounded-control border border-line p-2 text-xs">
							<p class="font-semibold">{r.author || 'Unknown'}</p>
							<p class="break-words whitespace-pre-wrap select-text">{r.contents}</p>
						</div>
					{/each}
				</div>
			{/if}

			{#if a.problems.length}
				<div class="flex flex-col gap-2 rounded-control bg-info-bg p-2 text-xs">
					<p class="flex items-center gap-1 font-semibold">
						<TriangleAlert size={14} aria-hidden="true" />Needs repair
					</p>
					<ul class="list-disc pl-4">
						{#each a.problems as p (p)}
							<li>It {PROBLEM_TEXT[p]}.</li>
						{/each}
					</ul>
					{#if tab.flags.canAnnotate}
						<button type="button" class="button gap-1 self-start" onclick={() => void repair(tab)}>
							<Wrench size={14} aria-hidden="true" />Repair annotations…
						</button>
					{/if}
				</div>
			{/if}
		</aside>
		{#each EDGES as edge (edge)}
			<div class="panel-handle" data-edge={edge} aria-hidden="true" onpointerdown={(e) => onHandleDown(e, edge)}></div>
		{/each}
	</div>
{/if}
