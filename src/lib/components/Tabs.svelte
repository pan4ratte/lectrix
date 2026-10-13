<script lang="ts">
	// Document tabs: click to switch, middle-click or the close button to close. A dot marks
	// unsaved changes. A tab is dragged along the row to reorder it (it follows the pointer
	// and the others make way; Ctrl+Shift+Left/Right from the keyboard), and off the row to
	// move it to another window: dropped on one of Lectrix's windows it joins that one's
	// tabs, anywhere else it gets a window of its own (section 8). Its context menu offers
	// the same moves. While the start screen shows over the tabs (the title bar's icon and
	// name), none is selected; the one it was shown over keeps the row's place in the Tab
	// order.
	import { FileText, Layers, X } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';
	import { tick } from 'svelte';

	import { APP_NAME } from '#lib/config.ts';
	import { listWindows, type WindowEntry } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { DRAG_THRESHOLD, dragShift, dropIndex, reordered, tornOff, type Span } from './tabdrag.ts';

	let strip: HTMLDivElement | undefined = $state();

	// ----- dragging -----

	interface TabDrag {
		id: number;
		pointer: number;
		startX: number;
		startY: number;
		/** Where the tab is held, from its left edge. */
		grab: number;
		x: number;
		y: number;
		/** Past the threshold: a drag, not a click. */
		moving: boolean;
		/** Pulled off the row: on its way to another window. */
		off: boolean;
		/** How far the tab is from its place in the row, under the pointer. */
		shift: number;
	}

	// Tracked on the window: the pointer leaves the row, and the window, with the tab. (The
	// row captures the pointer meanwhile, not the tab, whose node moves as it is reordered.)
	let drag = $state<TabDrag | null>(null);
	const dragged = $derived(drag?.moving ? drag : null);
	const draggedTab = $derived(dragged ? (app.tabs.find((t) => t.id === dragged.id) ?? null) : null);

	$effect(() => {
		document.documentElement.classList.toggle('tab-dragging', dragged !== null);
	});

	function onPointerDown(event: PointerEvent, id: number) {
		if (event.button === 1) {
			event.preventDefault();
			void app.closeTab(id);
			return;
		}
		if (event.button !== 0) return;
		if ((event.target as HTMLElement).closest('[data-close]')) return;
		app.activate(id);
		const left = (event.currentTarget as HTMLElement).getBoundingClientRect().left;
		drag = {
			id,
			pointer: event.pointerId,
			startX: event.clientX,
			startY: event.clientY,
			grab: event.clientX - left,
			x: event.clientX,
			y: event.clientY,
			moving: false,
			off: false,
			shift: 0
		};
	}

	function onPointerMove(event: PointerEvent) {
		if (!drag || !strip) return;
		if (!drag.moving) {
			if (Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < DRAG_THRESHOLD) return;
			drag.moving = true;
			try {
				strip.setPointerCapture(drag.pointer);
			} catch {
				// The pointer is gone already; the window's listeners still end the drag.
			}
		}
		drag.x = event.clientX;
		drag.y = event.clientY;
		drag.off = tornOff({ x: drag.x, y: drag.y }, strip.getBoundingClientRect(), window.innerWidth);
		if (drag.off) drag.shift = 0;
		else follow(drag, strip);
	}

	/** Keeps the dragged tab under the pointer, and moves it past the neighbours it has
	 * passed. */
	function follow(drag: TabDrag, strip: HTMLElement) {
		const from = app.tabs.findIndex((t) => t.id === drag.id);
		// Each tab's place in the row: the dragged one's without its shift.
		const spans: Span[] = [...strip.querySelectorAll<HTMLElement>('[data-tab]')].map((el, i) => {
			const r = el.getBoundingClientRect();
			const shift = i === from ? drag.shift : 0;
			return { left: r.left - shift, right: r.right - shift };
		});
		const own = spans[from];
		const last = spans[spans.length - 1];
		if (!own || !last) return;
		const row = { left: spans[0]!.left, right: last.right };
		const shift = dragShift(drag.x, drag.grab, own, row);
		const to = dropIndex(spans, from, { left: own.left + shift, right: own.right + shift });
		if (to === from) {
			drag.shift = shift;
			return;
		}
		// Its place changes with the order; the shift is from the new one, so the tab stays
		// where it is on screen.
		app.moveTab(from, to);
		drag.shift = dragShift(drag.x, drag.grab, reordered(spans, from, to)[to] ?? own, row);
	}

	function endDrag() {
		const pointer = drag?.pointer;
		drag = null;
		if (pointer !== undefined && strip?.hasPointerCapture(pointer)) strip.releasePointerCapture(pointer);
	}

	function onPointerUp() {
		const dropped = drag;
		endDrag();
		// Off the row: into the Lectrix window it was dropped on, else a window of its own
		// there (unless it is all this window shows: that would be this window again).
		if (dropped?.moving && dropped.off) {
			void app.moveToWindow(dropped.id, { kind: 'cursor', orNew: !app.isOnlyTab(dropped.id) });
		}
	}

	/** Esc drops the tab where it is in the row, and it stays in this window. */
	function onDragKey(event: KeyboardEvent) {
		if (event.key !== 'Escape' || !dragged) return;
		event.preventDefault();
		event.stopPropagation();
		endDrag();
	}

	// ----- keyboard -----

	function onCombineKeyDown(event: KeyboardEvent) {
		if (event.key === 'Delete') {
			event.preventDefault();
			void app.closeCombine();
		} else if (isPick(event)) {
			event.preventDefault();
			app.openCombine();
		}
	}

	/** Enter or Space on a tab itself (not its close button): from the start screen, where
	 * no tab is the one shown, that goes back to it. */
	function isPick(event: KeyboardEvent) {
		return (event.key === 'Enter' || event.key === ' ') && event.target === event.currentTarget;
	}

	/** Moves a tab one place along the row, keeping the keyboard focus on it. */
	async function step(id: number, direction: 1 | -1) {
		const i = app.tabs.findIndex((t) => t.id === id);
		app.moveTab(i, i + direction);
		await tick();
		strip?.querySelector<HTMLElement>(`[data-tab="${id}"]`)?.focus();
	}

	function onKeyDown(event: KeyboardEvent, id: number) {
		const i = app.tabs.findIndex((t) => t.id === id);
		if (event.ctrlKey && event.shiftKey && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
			event.preventDefault();
			void step(id, event.key === 'ArrowLeft' ? -1 : 1);
			return;
		}
		if ((event.key === 'ArrowLeft' || event.key === 'ArrowRight') && !event.altKey && !event.ctrlKey) {
			event.preventDefault();
			const next = app.tabs[(i + (event.key === 'ArrowLeft' ? -1 : 1) + app.tabs.length) % app.tabs.length];
			if (next) {
				app.activate(next.id);
				strip?.querySelector<HTMLElement>(`[data-tab="${next.id}"]`)?.focus();
			}
		} else if (event.key === 'Delete') {
			event.preventDefault();
			void app.closeTab(id);
		} else if (isPick(event)) {
			event.preventDefault();
			app.activate(id);
		}
	}

	// ----- context menu -----

	/** The tab that was right-clicked: a document's, or (null) the Combine view's. */
	let contextTab = $state<number | null>(null);
	const contextIndex = $derived(contextTab === null ? -1 : app.tabs.findIndex((t) => t.id === contextTab));
	/** Lectrix's other windows, asked for as the menu opens. */
	let otherWindows = $state<WindowEntry[]>([]);

	function onContextMenu(id: number | null) {
		contextTab = id;
		otherWindows = [];
		if (id === null) return;
		void listWindows()
			.then((windows) => (otherWindows = windows))
			.catch(() => {});
	}

	/** A window as the menu names it: by what it shows, without the app's name after it. */
	function windowName(title: string): string {
		const suffix = ` - ${APP_NAME}`;
		return title.endsWith(suffix) ? title.slice(0, -suffix.length) : title || APP_NAME;
	}
</script>

<svelte:window
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={endDrag}
	onblur={endDrag}
	onkeydowncapture={onDragKey}
/>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<div
				{...props}
				bind:this={strip}
				class="flex h-full min-w-0 items-center gap-0.5"
				role="tablist"
				aria-label="Open documents"
			>
				{#each app.tabs as tab (tab.id)}
					{@const active = tab.id === app.activeId && !app.combineActive && !app.startShown}
					{@const reachable = active || (app.startShown && tab.id === app.activeId)}
					{@const held = dragged?.id === tab.id ? dragged : null}
					<div
						data-tab={tab.id}
						class="doc-tab group relative flex h-8 max-w-56 min-w-28 shrink items-center gap-1 pr-[4px] pl-3"
						class:doc-tab-held={held && !held.off}
						class:opacity-50={held?.off}
						style:transform={held && !held.off ? `translateX(${held.shift}px)` : undefined}
						role="tab"
						tabindex={reachable ? 0 : -1}
						aria-selected={active}
						data-tip={tab.untitled ? 'Not saved yet' : tab.path}
						onpointerdown={(e) => onPointerDown(e, tab.id)}
						onkeydown={(e) => onKeyDown(e, tab.id)}
						oncontextmenu={() => onContextMenu(tab.id)}
					>
						<span class="truncate" class:text-fg-muted={!active}>{tab.name}</span>
						{#if tab.saving}
							<span
								class="size-3 shrink-0 animate-spin rounded-full border-2 border-fg-muted border-t-transparent"
								aria-label="saving"
								data-tip="Saving…"
								role="img"
								data-saving
							></span>
						{:else if tab.state.dirty}
							<span
								class="size-2 shrink-0 rounded-full bg-fg"
								aria-label="unsaved changes"
								data-tip="Unsaved changes"
								role="img"
								data-unsaved
							></span>
						{/if}
						<button
							type="button"
							data-close
							class="icon-button tab-close ml-auto shrink-0"
							aria-label="Close {tab.name}"
							data-tip={app.hint('Close', 'closeTab')}
							tabindex={reachable ? 0 : -1}
							onclick={() => void app.closeTab(tab.id)}
						>
							<X size={14} aria-hidden="true" />
						</button>
					</div>
				{/each}
				{#if app.combine}
					{@const active = app.combineActive}
					{@const reachable = active || (app.startShown && app.tabs.length === 0)}
					<div
						data-combine-tab
						class="doc-tab group relative flex h-8 max-w-56 min-w-28 shrink items-center gap-2 pr-[4px] pl-3"
						role="tab"
						tabindex={reachable ? 0 : -1}
						aria-selected={active}
						data-tip="Combine files"
						onpointerdown={(e) => {
							if (e.button === 1) {
								e.preventDefault();
								void app.closeCombine();
							} else if (e.button === 0 && !(e.target as HTMLElement).closest('[data-close]')) {
								app.openCombine();
							}
						}}
						onkeydown={onCombineKeyDown}
						oncontextmenu={() => onContextMenu(null)}
					>
						<Layers size={14} aria-hidden="true" class="shrink-0" />
						<span class="truncate" class:text-fg-muted={!active}>Combine files</span>
						<button
							type="button"
							data-close
							class="icon-button tab-close ml-auto shrink-0"
							aria-label="Close Combine files"
							data-tip={app.hint('Close', 'closeTab')}
							tabindex={reachable ? 0 : -1}
							onclick={() => void app.closeCombine()}
						>
							<X size={14} aria-hidden="true" />
						</button>
					</div>
				{/if}
			</div>
		{/snippet}
	</ContextMenu.Trigger>
	<ContextMenu.Portal>
		<ContextMenu.Content class="menu-content">
			{#if contextTab === null}
				<ContextMenu.Item class="menu-item" onSelect={() => void app.closeCombine()}>
					Close<span class="menu-shortcut">{app.shortcut('closeTab')}</span>
				</ContextMenu.Item>
			{:else}
				{@const id = contextTab}
				<ContextMenu.Item
					class="menu-item"
					disabled={contextIndex < 0 || app.isOnlyTab(id)}
					onSelect={() => void app.moveToWindow(id, { kind: 'new' })}
				>
					Move to new window
				</ContextMenu.Item>
				{#each otherWindows as other (other.label)}
					<ContextMenu.Item
						class="menu-item"
						disabled={contextIndex < 0}
						onSelect={() => void app.moveToWindow(id, { kind: 'window', label: other.label })}
					>
						<span class="max-w-[320px] truncate">Move to window: {windowName(other.title)}</span>
					</ContextMenu.Item>
				{/each}
				<ContextMenu.Separator class="menu-separator" />
				<ContextMenu.Item class="menu-item" disabled={contextIndex <= 0} onSelect={() => void step(id, -1)}>
					Move left<span class="menu-shortcut">Ctrl+Shift+Left</span>
				</ContextMenu.Item>
				<ContextMenu.Item
					class="menu-item"
					disabled={contextIndex < 0 || contextIndex >= app.tabs.length - 1}
					onSelect={() => void step(id, 1)}
				>
					Move right<span class="menu-shortcut">Ctrl+Shift+Right</span>
				</ContextMenu.Item>
				<ContextMenu.Separator class="menu-separator" />
				<ContextMenu.Item class="menu-item" disabled={contextIndex < 0} onSelect={() => void app.closeTab(id)}>
					Close<span class="menu-shortcut">{app.shortcut('closeTab')}</span>
				</ContextMenu.Item>
			{/if}
		</ContextMenu.Content>
	</ContextMenu.Portal>
</ContextMenu.Root>

<!-- The tab pulled off its row, following the pointer while it is over this window. -->
{#if dragged?.off && draggedTab}
	<div class="panel-drag-ghost" style:left="{dragged.x + 12}px" style:top="{dragged.y + 12}px" aria-hidden="true">
		<FileText size={16} />
		<span class="max-w-[240px] truncate">{draggedTab.name}</span>
	</div>
{/if}
