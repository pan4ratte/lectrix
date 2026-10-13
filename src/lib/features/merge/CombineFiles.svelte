<script lang="ts">
	// The Combine view's files (section 6.4), in the order they are listed: each with the
	// colour that marks its pages in the grid, how many of its pages are in the result, and
	// buttons that put its removed pages back or leave it out. A file is dragged up or down
	// the list to another place (Alt+Shift+Up/Down from the keyboard), and its pages go with
	// it in the grid; the arrow keys move between the files.
	import { ArchiveRestore, X } from '@lucide/svelte';
	import { tick } from 'svelte';

	import type { CombineState } from './combine.svelte.ts';
	import { rowGapAt } from './pages.ts';

	let { combine }: { combine: CombineState } = $props();

	const DRAG_THRESHOLD = 6;

	let list: HTMLUListElement | undefined = $state();
	const movable = $derived(combine.sources.length > 1 && !combine.running);

	function pagesText(count: number, total: number) {
		if (count === total) return total === 1 ? '1 page' : `${total.toLocaleString()} pages`;
		if (count === 0) return 'Left out';
		return `${count.toLocaleString()} of ${total.toLocaleString()} pages`;
	}

	function rows(): HTMLElement[] {
		return list ? [...list.querySelectorAll<HTMLElement>('[data-file]')] : [];
	}

	// ----- dragging a file to another place -----

	let press: { id: number; x: number; y: number } | null = null;
	/** The file being dragged: where the pointer is, the gap of the list it would go into,
	 * and where that gap is in the list's content (null where it would stay as it is). */
	let drag = $state<{ id: number; name: string; x: number; y: number; gap: number; line: number | null } | null>(null);
	let autoScroll = 0;

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0 || !movable || !list) return;
		const target = event.target as HTMLElement;
		// A row's buttons are their own.
		if (target.closest('button')) return;
		const row = target.closest<HTMLElement>('[data-file]');
		if (!row) return;
		press = { id: Number(row.dataset.file), x: event.clientX, y: event.clientY };
		// (The list holds the pointer from here on, so the press would not focus the row.)
		row.focus({ preventScroll: true });
		list.setPointerCapture(event.pointerId);
	}

	function onPointerMove(event: PointerEvent) {
		if (!press) return;
		if (!drag) {
			if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_THRESHOLD) return;
			const source = combine.source(press.id);
			if (!source) return;
			drag = { id: source.id, name: source.name, x: event.clientX, y: event.clientY, gap: 0, line: null };
			autoScroll = requestAnimationFrame(autoScrollStep);
		}
		drag.x = event.clientX;
		drag.y = event.clientY;
		updateGap();
	}

	/** The gap under the pointer, and the line that shows it. */
	function updateGap() {
		if (!drag) return;
		const all = rows();
		const gap = rowGapAt(
			all.map((row) => row.getBoundingClientRect()),
			drag.y
		);
		const from = combine.sources.findIndex((s) => s.id === drag?.id);
		const last = all[all.length - 1];
		const at = all[gap];
		drag.gap = gap;
		// The gaps on either side of the file leave it where it is.
		if (gap === from || gap === from + 1 || !last) drag.line = null;
		else drag.line = at ? at.offsetTop : last.offsetTop + last.offsetHeight;
	}

	/** Scrolls the list while a file is dragged near its top or bottom edge. */
	function autoScrollStep() {
		if (!drag || !list) return;
		const rect = list.getBoundingClientRect();
		const edge = 24;
		let dy = 0;
		if (drag.y < rect.top + edge) dy = -Math.min(12, rect.top + edge - drag.y);
		else if (drag.y > rect.bottom - edge) dy = Math.min(12, drag.y - (rect.bottom - edge));
		if (dy !== 0) {
			list.scrollTop += dy;
			updateGap();
		}
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	function onPointerUp(event: PointerEvent) {
		if (list?.hasPointerCapture(event.pointerId)) list.releasePointerCapture(event.pointerId);
		const dropped = drag;
		stopDrag();
		if (dropped && dropped.line !== null) combine.moveFile(dropped.id, dropped.gap);
	}

	/** Ends a press or a drag without moving anything. */
	function stopDrag() {
		cancelAnimationFrame(autoScroll);
		press = null;
		drag = null;
	}

	/** Esc drops the file where it was. */
	function onDragKey(event: KeyboardEvent) {
		if (event.key !== 'Escape' || !drag) return;
		event.preventDefault();
		event.stopPropagation();
		stopDrag();
	}

	// ----- keyboard -----

	function focusRow(id: number | undefined) {
		if (id !== undefined) list?.querySelector<HTMLElement>(`[data-file="${id}"]`)?.focus();
	}

	async function onKeyDown(event: KeyboardEvent, id: number) {
		// (Keys on a row's buttons are theirs.)
		if (event.target !== event.currentTarget) return;
		if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
		const direction = event.key === 'ArrowUp' ? -1 : 1;
		if (event.altKey && event.shiftKey && !event.ctrlKey) {
			event.preventDefault();
			if (!movable) return;
			combine.nudgeFile(id, direction);
			// The row's node moves with it: the keyboard focus stays on the file.
			await tick();
			focusRow(id);
		} else if (!event.altKey && !event.shiftKey && !event.ctrlKey) {
			event.preventDefault();
			const index = combine.sources.findIndex((s) => s.id === id);
			focusRow(combine.sources[index + direction]?.id);
		}
	}
</script>

<svelte:window onkeydowncapture={onDragKey} />

<ul
	bind:this={list}
	class="relative min-h-0 flex-1 overflow-y-auto px-2"
	aria-label="Files"
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={stopDrag}
	onlostpointercapture={stopDrag}
>
	{#each combine.sources as source (source.id)}
		{@const count = combine.pagesFrom(source.id)}
		{@const pages = pagesText(count, source.pages.length)}
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<li
			class="combine-file group flex items-center gap-2 rounded-control py-1.5 pr-1 pl-2 hover:bg-hover"
			class:opacity-40={drag?.id === source.id}
			data-file={source.id}
			tabindex="0"
			aria-label="{source.name}, {pages}"
			aria-keyshortcuts={movable ? 'Alt+Shift+ArrowUp Alt+Shift+ArrowDown' : undefined}
			data-tip={movable ? 'Drag to reorder (Alt+Shift+Up or Down)' : undefined}
			onkeydown={(event) => void onKeyDown(event, source.id)}
		>
			<span
				class="size-2.5 shrink-0 rounded-full"
				style:background="var(--lectrix-source-{source.color})"
				aria-hidden="true"
			></span>
			<span class="min-w-0 flex-1">
				<span class="block truncate" data-tip={source.path}>{source.name}</span>
				<span class="block text-xs text-fg-muted">{pages}</span>
			</span>
			{#if count < source.pages.length}
				<button
					type="button"
					class="icon-button size-7 shrink-0"
					aria-label="Put back the pages of {source.name} that were removed"
					data-tip="Put back removed pages"
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
					data-tip="Leave out this file"
					onclick={() => combine.removeFile(source.id)}
				>
					<X size={14} aria-hidden="true" />
				</button>
			{/if}
		</li>
	{/each}
	{#if drag && drag.line !== null}
		<li class="combine-file-drop-line" style:top="{Math.max(0, drag.line - 1)}px" aria-hidden="true"></li>
	{/if}
</ul>

{#if drag}
	<div class="drag-ghost" style:left="{drag.x + 14}px" style:top="{drag.y + 14}px" aria-hidden="true">{drag.name}</div>
{/if}
