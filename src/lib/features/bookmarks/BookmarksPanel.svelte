<script lang="ts">
	// Bookmarks panel (section 6.2): the outline as a tree. A click follows a bookmark; the
	// arrow keys move through the tree (ARIA tree pattern) and Enter follows; F2 or a
	// double-click renames; dragging or Alt+Shift+arrows reorder and nest; Del deletes.
	// Rows are virtualized: outlines can have thousands of bookmarks.
	//
	// Dragging uses pointer events: with Tauri's file drop enabled, the webview gets no
	// HTML5 drag-and-drop events.
	import { BookmarkPlus, ChevronRight, TriangleAlert } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';
	import { tick } from 'svelte';

	import { chain } from '#lib/components/chain.ts';
	import { ClickCounter } from '#lib/components/clicks.ts';
	import { stopUnlessShortcut } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import {
		addBookmark,
		copyLink,
		deleteBookmark,
		goToBookmark,
		keyboardMoveBookmark,
		moveBookmark,
		renameBookmark,
		setDestinationHere,
		setOpen,
		startRename
	} from './actions.ts';
	import { describeTarget, dropPosition, dropZone, locate, visibleRows, type DropZone, type Position, type Row } from './tree.ts';

	let { tab }: { tab: DocTab } = $props();

	const ROW = 28;
	const INDENT = 16;
	const DRAG_THRESHOLD = 4;

	let list: HTMLDivElement | undefined = $state();
	let scrollTop = $state(0);
	let viewportH = $state(0);

	const rows = $derived(visibleRows(tab.outline.items));
	const editable = $derived(tab.canEditBookmarks);
	const slice = $derived.by(() => {
		const first = Math.max(0, Math.floor(scrollTop / ROW) - 8);
		const last = Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW) + 8);
		return rows.slice(first, last).map((row, i) => ({ row, top: (first + i) * ROW }));
	});
	const selectedIndex = $derived(rows.findIndex((r) => r.bookmark.id === tab.selectedBookmark));

	function ensureVisible(index: number) {
		if (!list || index < 0) return;
		const top = index * ROW;
		if (top < list.scrollTop) list.scrollTop = top;
		else if (top + ROW > list.scrollTop + viewportH) list.scrollTop = top + ROW - viewportH;
	}

	// Keep the selected bookmark in view (it may be selected from outside: Ctrl+B, moves).
	$effect(() => {
		const index = selectedIndex;
		void tick().then(() => ensureVisible(index));
	});

	function select(index: number) {
		const row = rows[Math.max(0, Math.min(rows.length - 1, index))];
		if (!row) return;
		tab.selectedBookmark = row.bookmark.id;
	}

	function focusList() {
		list?.focus({ preventScroll: true });
	}

	// ----- keyboard -----

	function onKeyDown(event: KeyboardEvent) {
		if (event.target instanceof HTMLInputElement) return;
		if (drag && event.key === 'Escape') {
			cancelDrag();
			event.preventDefault();
			return;
		}
		const row = rows[selectedIndex];
		let handled = true;
		if (event.altKey && event.shiftKey && row) {
			const moves: Record<string, 'up' | 'down' | 'in' | 'out'> = {
				ArrowUp: 'up',
				ArrowDown: 'down',
				ArrowRight: 'in',
				ArrowLeft: 'out'
			};
			const move = moves[event.key];
			if (move) void keyboardMoveBookmark(tab, row.bookmark.id, move);
			else handled = false;
		} else if (event.altKey || event.ctrlKey || event.metaKey) {
			handled = false;
		} else {
			switch (event.key) {
				case 'ArrowDown':
					select(selectedIndex < 0 ? 0 : selectedIndex + 1);
					break;
				case 'ArrowUp':
					select(selectedIndex < 0 ? 0 : selectedIndex - 1);
					break;
				case 'Home':
					select(0);
					break;
				case 'End':
					select(rows.length - 1);
					break;
				case 'ArrowRight':
					if (!row) select(0);
					else if (row.bookmark.children.length && !row.bookmark.open) setOpen(tab, row.bookmark, true);
					else if (row.bookmark.open) select(selectedIndex + 1);
					break;
				case 'ArrowLeft':
					if (!row) select(0);
					else if (row.bookmark.open) setOpen(tab, row.bookmark, false);
					else if (row.parent !== null) tab.selectedBookmark = row.parent;
					break;
				case 'Enter':
				case ' ':
					if (row) goToBookmark(tab, row.bookmark.id);
					break;
				case 'F2':
					if (row) startRename(tab, row.bookmark.id);
					break;
				case 'Delete':
					if (row) void deleteBookmark(tab, row.bookmark.id).then(focusList);
					break;
				default:
					handled = false;
			}
		}
		if (handled) {
			event.preventDefault();
			// Keep Alt+arrows and the like from reaching the window's shortcuts.
			event.stopPropagation();
		}
	}

	// ----- pointer: click, toggle, drag -----

	let press: { id: number; x: number; y: number } | null = null;
	let drag = $state<{
		id: number;
		title: string;
		x: number;
		y: number;
		drop: { top: number; depth: number; zone: DropZone; to: Position; rowId: number } | null;
	} | null>(null);
	let autoScroll = 0;
	let lastPointer = { x: 0, y: 0 };

	function rowAt(clientY: number): { row: Row; index: number; offset: number } | null {
		if (!list) return null;
		const y = clientY - list.getBoundingClientRect().top + list.scrollTop;
		const index = Math.floor(y / ROW);
		const row = rows[index];
		return row ? { row, index, offset: y - index * ROW } : null;
	}

	const clickCounter = new ClickCounter();

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0 || !list) return;
		const target = event.target as HTMLElement;
		if (target.closest('input')) return;
		const rowEl = target.closest<HTMLElement>('[data-row]');
		if (!rowEl) return;
		const id = Number(rowEl.dataset.row);
		event.preventDefault();
		focusList();
		if (target.closest('[data-toggle]')) {
			const found = locate(tab.outline.items, id);
			if (found) setOpen(tab, found.bookmark, !found.bookmark.open);
			return;
		}
		if (clickCounter.count(event) === 2) {
			startRename(tab, id);
			return;
		}
		press = { id, x: event.clientX, y: event.clientY };
		list.setPointerCapture(event.pointerId);
	}

	function onPointerMove(event: PointerEvent) {
		lastPointer = { x: event.clientX, y: event.clientY };
		if (!press) return;
		if (!drag) {
			const moved = Math.hypot(event.clientX - press.x, event.clientY - press.y) > DRAG_THRESHOLD;
			if (!moved || !editable || tab.renamingBookmark !== null) return;
			const found = locate(tab.outline.items, press.id);
			if (!found) return;
			drag = { id: press.id, title: found.bookmark.title, x: event.clientX, y: event.clientY, drop: null };
			autoScroll = requestAnimationFrame(autoScrollStep);
		}
		updateDrop(event.clientX, event.clientY);
	}

	function updateDrop(clientX: number, clientY: number) {
		if (!drag) return;
		drag.x = clientX;
		drag.y = clientY;
		const hit = rowAt(clientY);
		if (!hit) {
			drag.drop = null;
			return;
		}
		const zone = dropZone(hit.offset, ROW);
		const to = dropPosition(tab.outline.items, drag.id, hit.row, zone);
		if (!to) {
			drag.drop = null;
			return;
		}
		const intoOpen = zone === 'after' && hit.row.bookmark.open && hit.row.bookmark.children.length > 0;
		drag.drop = {
			top: zone === 'before' ? hit.index * ROW : (hit.index + 1) * ROW,
			depth: hit.row.depth + (intoOpen ? 1 : 0),
			zone,
			to,
			rowId: hit.row.bookmark.id
		};
	}

	/** Scrolls while a bookmark is dragged near the top or bottom edge. */
	function autoScrollStep() {
		if (!drag || !list) return;
		const rect = list.getBoundingClientRect();
		const edge = 24;
		let dy = 0;
		if (lastPointer.y < rect.top + edge) dy = -Math.min(16, rect.top + edge - lastPointer.y);
		else if (lastPointer.y > rect.bottom - edge) dy = Math.min(16, lastPointer.y - (rect.bottom - edge));
		if (dy !== 0) {
			list.scrollTop += dy;
			updateDrop(lastPointer.x, lastPointer.y);
		}
		autoScroll = requestAnimationFrame(autoScrollStep);
	}

	function endPress(event: PointerEvent) {
		if (list?.hasPointerCapture(event.pointerId)) list.releasePointerCapture(event.pointerId);
		cancelAnimationFrame(autoScroll);
		press = null;
	}

	function onPointerUp(event: PointerEvent) {
		const pressed = press;
		const dragged = drag;
		endPress(event);
		drag = null;
		if (dragged) {
			if (dragged.drop) void moveBookmark(tab, dragged.id, dragged.drop.to);
		} else if (pressed) {
			tab.selectedBookmark = pressed.id;
			goToBookmark(tab, pressed.id);
		}
	}

	function cancelDrag() {
		drag = null;
		press = null;
		cancelAnimationFrame(autoScroll);
	}

	// ----- rename -----

	function focusSelect(node: HTMLInputElement) {
		node.focus();
		node.select();
	}

	function onRenameKey(event: KeyboardEvent, id: number) {
		stopUnlessShortcut(event);
		if (event.key === 'Enter') {
			event.preventDefault();
			void renameBookmark(tab, id, (event.currentTarget as HTMLInputElement).value);
			focusList();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			tab.renamingBookmark = null;
			focusList();
		}
	}

	function onRenameBlur(event: FocusEvent, id: number) {
		if (tab.renamingBookmark === id) void renameBookmark(tab, id, (event.currentTarget as HTMLInputElement).value);
	}

	// ----- context menu -----

	const menuBookmark = $derived(selectedIndex >= 0 ? rows[selectedIndex]!.bookmark : null);

	function onContextMenu(event: MouseEvent) {
		const hit = rowAt(event.clientY);
		if (hit) tab.selectedBookmark = hit.row.bookmark.id;
	}

	function onScroll() {
		if (list) scrollTop = list.scrollTop;
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex shrink-0 items-center gap-1 border-b border-line px-2 py-[6px]">
		<h2 class="flex-1 px-1 text-xs font-semibold tracking-wide text-fg-muted uppercase">Bookmarks</h2>
		<button
			type="button"
			class="icon-button"
			aria-label="Add bookmark (Ctrl+B)"
			title="Add bookmark (Ctrl+B)"
			disabled={!editable}
			onclick={() => void addBookmark(tab)}
		>
			<BookmarkPlus size={16} aria-hidden="true" />
		</button>
	</div>

	{#if tab.outline.damaged}
		<p class="mx-2 mb-2 flex gap-2 rounded-control bg-danger-bg p-2 text-xs" role="note">
			<TriangleAlert size={14} class="mt-0.5 shrink-0" aria-hidden="true" />
			These bookmarks are damaged. Lectrix shows them as far as it can, but can’t change them.
		</p>
	{/if}

	{#if rows.length === 0}
		<p class="px-3 py-2 text-sm text-fg-muted">
			No bookmarks yet. Press Ctrl+B to add one for the current page.
		</p>
	{:else}
		<ContextMenu.Root>
			<ContextMenu.Trigger>
				{#snippet child({ props })}
					<div
						{...props}
						bind:this={list}
						bind:clientHeight={viewportH}
						class="tree relative min-h-0 flex-1 overflow-auto px-1 outline-none"
						role="tree"
						aria-label="Bookmarks"
						tabindex="0"
						aria-activedescendant={tab.selectedBookmark !== null && selectedIndex >= 0
							? `bookmark-${tab.selectedBookmark}`
							: undefined}
						onscroll={onScroll}
						onkeydown={onKeyDown}
						onpointerdown={chain(props, 'onpointerdown', onPointerDown)}
						onpointermove={chain(props, 'onpointermove', onPointerMove)}
						onpointerup={chain(props, 'onpointerup', onPointerUp)}
						onpointercancel={chain(props, 'onpointercancel', (e: PointerEvent) => {
							endPress(e);
							cancelDrag();
						})}
						oncontextmenu={chain(props, 'oncontextmenu', onContextMenu)}
					>
						<div class="relative" style:height="{rows.length * ROW}px">
							{#each slice as { row, top } (row.bookmark.id)}
								{@const b = row.bookmark}
								<div
									id="bookmark-{b.id}"
									data-row={b.id}
									class="tree-row"
									class:tree-drop-inside={drag?.drop?.zone === 'inside' && drag.drop.rowId === b.id}
									class:opacity-50={drag?.id === b.id}
									style:top="{top}px"
									style:padding-left="{row.depth * INDENT + 4}px"
									role="treeitem"
									aria-level={row.depth + 1}
									aria-setsize={row.siblings}
									aria-posinset={row.index + 1}
									aria-expanded={b.children.length ? b.open : undefined}
									aria-selected={tab.selectedBookmark === b.id}
									title="{b.title}&#10;{describeTarget(b.target, tab.labels)}"
								>
									{#if b.children.length}
										<span data-toggle class="tree-toggle" class:rotate-90={b.open} aria-hidden="true">
											<ChevronRight size={14} />
										</span>
									{:else}
										<span class="w-5 shrink-0" aria-hidden="true"></span>
									{/if}
									{#if b.color}
										<span class="bookmark-color" style:background-color={b.color} aria-hidden="true"></span>
									{/if}
									{#if tab.renamingBookmark === b.id}
										<input
											class="tree-rename"
											value={b.title}
											aria-label="Bookmark title"
											use:focusSelect
											onkeydown={(e) => onRenameKey(e, b.id)}
											onblur={(e) => onRenameBlur(e, b.id)}
										/>
									{:else}
										<span class="truncate" class:font-semibold={b.bold} class:italic={b.italic}>{b.title || '(untitled)'}</span>
									{/if}
								</div>
							{/each}
							{#if drag?.drop && drag.drop.zone !== 'inside'}
								<div
									class="tree-drop-line"
									style:top="{drag.drop.top - 1}px"
									style:left="{drag.drop.depth * INDENT + 8}px"
									aria-hidden="true"
								></div>
							{/if}
						</div>
					</div>
				{/snippet}
			</ContextMenu.Trigger>
			<ContextMenu.Portal>
				<ContextMenu.Content class="menu-content">
					{#if menuBookmark}
						{@const b = menuBookmark}
						<ContextMenu.Item class="menu-item" onSelect={() => goToBookmark(tab, b.id)}>
							Go to bookmark<span class="menu-shortcut">Enter</span>
						</ContextMenu.Item>
						{#if b.target.kind === 'uri'}
							{@const uri = b.target.uri}
							<ContextMenu.Item class="menu-item" onSelect={() => void copyLink(uri)}>Copy link</ContextMenu.Item>
						{/if}
						<ContextMenu.Item class="menu-item" onSelect={() => (app.inspectorOpen = true)}>
							Properties
						</ContextMenu.Item>
						<ContextMenu.Separator class="menu-separator" />
						<ContextMenu.Item class="menu-item" disabled={!editable} onSelect={() => startRename(tab, b.id)}>
							Rename<span class="menu-shortcut">F2</span>
						</ContextMenu.Item>
						<ContextMenu.Item
							class="menu-item"
							disabled={!editable}
							onSelect={() => void setDestinationHere(tab, b.id)}
						>
							Set destination to current view
						</ContextMenu.Item>
						<ContextMenu.Separator class="menu-separator" />
						<ContextMenu.Item
							class="menu-item"
							disabled={!editable}
							onSelect={() => void keyboardMoveBookmark(tab, b.id, 'up')}
						>
							Move up<span class="menu-shortcut">Alt+Shift+Up</span>
						</ContextMenu.Item>
						<ContextMenu.Item
							class="menu-item"
							disabled={!editable}
							onSelect={() => void keyboardMoveBookmark(tab, b.id, 'down')}
						>
							Move down<span class="menu-shortcut">Alt+Shift+Down</span>
						</ContextMenu.Item>
						<ContextMenu.Item
							class="menu-item"
							disabled={!editable}
							onSelect={() => void keyboardMoveBookmark(tab, b.id, 'in')}
						>
							Nest under previous<span class="menu-shortcut">Alt+Shift+Right</span>
						</ContextMenu.Item>
						<ContextMenu.Item
							class="menu-item"
							disabled={!editable}
							onSelect={() => void keyboardMoveBookmark(tab, b.id, 'out')}
						>
							Move out one level<span class="menu-shortcut">Alt+Shift+Left</span>
						</ContextMenu.Item>
						<ContextMenu.Separator class="menu-separator" />
						<ContextMenu.Item class="menu-item" disabled={!editable} onSelect={() => void addBookmark(tab)}>
							Add bookmark after this<span class="menu-shortcut">Ctrl+B</span>
						</ContextMenu.Item>
						<ContextMenu.Item
							class="menu-item"
							disabled={!editable}
							onSelect={() => void deleteBookmark(tab, b.id)}
						>
							Delete<span class="menu-shortcut">Del</span>
						</ContextMenu.Item>
					{/if}
				</ContextMenu.Content>
			</ContextMenu.Portal>
		</ContextMenu.Root>
	{/if}
</div>

{#if drag}
	<div class="drag-ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 8}px" aria-hidden="true">
		{drag.title}
	</div>
{/if}
