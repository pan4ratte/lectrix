<script lang="ts">
	// Document tabs: click to switch, middle-click or the close button to close. A dot marks
	// unsaved changes. A tab is dragged along the row to reorder it (it follows the pointer
	// and the others make way; Ctrl+Shift+Left/Right from the keyboard), and off the row to
	// move it to another window: dropped outside its window, on one of Lectrix's windows it
	// joins that one's tabs, anywhere else it gets a window of its own (section 8); dropped
	// inside its window, it stays. Dropped near an edge of the pages, it goes beside the tab
	// shown, as a joined tab (section 6.1, split view): beside the one shown before it, when
	// it was on screen itself. The tab on screen stays there while a tab is dragged; a tab is
	// shown when it is let go (a click, a drop). A joined tab shows its documents' names side by side, and
	// closing it closes each of them. Its context menu offers the same moves. While the start screen shows over
	// the tabs (the title bar's icon and name), none is selected; the one it was shown over
	// keeps the row's place in the Tab order. A tab grows into the row as it opens and shrinks
	// away as it closes.
	import { FileText, Layers, Lock, X } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';
	import { tick } from 'svelte';

	import { tooManyPanes } from '#lib/commands.ts';
	import { APP_NAME } from '#lib/config.ts';
	import { openProperties } from '#lib/features/properties/actions.ts';
	import { MAX_PANES, joinIndex } from '#lib/features/split/split.ts';
	import { toggleLock, toggleZoomLock } from '#lib/features/split/sync.ts';
	import { clearDrop, dropArea, tabDrop } from '#lib/features/split/tabdrop.svelte.ts';
	import { listWindows, type WindowEntry } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { TabGroup } from '#lib/stores/group.svelte.ts';

	import { DRAG_THRESHOLD, dragShift, dropIndex, reordered, tornOff, type Span } from './tabdrag.ts';
	import { tabIn, tabOut } from './tabmotion.ts';

	let strip: HTMLDivElement | undefined = $state();

	const groupOf = (key: number) => app.groups.find((g) => g.key === key) ?? null;

	// ----- dragging -----

	interface TabDrag {
		/** The dragged tab's key. */
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
		/** Pulled off the row: on its way to another window, or beside the tab shown. */
		off: boolean;
		/** How far the tab is from its place in the row, under the pointer. */
		shift: number;
		/** The tab it would go beside, dropped near an edge of the pages: the one on screen,
		 * or, when that is this one, the one shown before it. */
		before: TabGroup | null;
	}

	// Tracked on the window: the pointer leaves the row, and the window, with the tab. (The
	// row captures the pointer meanwhile, not the tab, whose node moves as it is reordered.)
	let drag = $state<TabDrag | null>(null);
	const dragged = $derived(drag?.moving ? drag : null);
	const draggedGroup = $derived(dragged ? groupOf(dragged.id) : null);

	$effect(() => {
		document.documentElement.classList.toggle('tab-dragging', dragged !== null);
	});

	function onPointerDown(event: PointerEvent, group: TabGroup) {
		if (event.button === 1) {
			event.preventDefault();
			void app.closeGroup(group);
			return;
		}
		if (event.button !== 0) return;
		if ((event.target as HTMLElement).closest('[data-close], [data-protected]')) return;
		const shown = app.active ? app.activeGroup : null;
		const before = shown && shown !== group ? shown : app.shownBefore(group);
		const left = (event.currentTarget as HTMLElement).getBoundingClientRect().left;
		drag = {
			id: group.key,
			pointer: event.pointerId,
			startX: event.clientX,
			startY: event.clientY,
			grab: event.clientX - left,
			x: event.clientX,
			y: event.clientY,
			moving: false,
			off: false,
			shift: 0,
			before
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
		if (drag.off) {
			drag.shift = 0;
			placeBeside(drag);
		} else {
			clearDrop();
			follow(drag, strip);
		}
	}

	/** Where the tab pulled off the row would go beside the tab it goes beside (`before`), if
	 * it is near an edge of the pages on screen and fits there (four panes at most; a joined
	 * tab does not go beside itself). */
	function placeBeside(drag: TabDrag) {
		const moving = groupOf(drag.id);
		const target = drag.before && app.groups.includes(drag.before) ? drag.before : moving;
		const area = app.active ? dropArea.element : null;
		clearDrop();
		if (!moving || !target || !area) return;
		const r = area.getBoundingClientRect();
		if (drag.y < r.top || drag.y > r.bottom) return;
		const at = joinIndex(drag.x - r.left, r.width, target.widths);
		if (at === null) return;
		const fits = moving === target ? !moving.joined && !target.full : target.views.length + moving.views.length <= MAX_PANES;
		if (!fits) return;
		tabDrop.at = at;
		tabDrop.panes = target.views.length;
	}

	/** Keeps the dragged tab under the pointer, and moves it past the neighbours it has
	 * passed. */
	function follow(drag: TabDrag, strip: HTMLElement) {
		const from = app.groups.findIndex((g) => g.key === drag.id);
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
		clearDrop();
		if (pointer !== undefined && strip?.hasPointerCapture(pointer)) strip.releasePointerCapture(pointer);
	}

	function onPointerUp() {
		const dropped = drag;
		const at = tabDrop.at;
		endDrag();
		const group = dropped ? groupOf(dropped.id) : null;
		if (!dropped || !group) return;
		// A click, or a drag along the row: the tab is shown.
		if (!dropped.moving || !dropped.off) {
			app.show(group);
			return;
		}
		// Near an edge of the pages shown: beside the tab it goes beside.
		if (at !== null) {
			joinBeside(group, dropped.before, at);
			return;
		}
		// Elsewhere in its window: it stays, and shows, as the press that started the drag
		// would have it.
		if (inWindow(dropped.x, dropped.y)) {
			app.show(group);
			return;
		}
		// Out of its window: into the Lectrix window it was dropped on, else a window of its
		// own there (unless it is all this window shows: that would be this window again).
		void app.moveGroupToWindow(group, { kind: 'cursor', orNew: !app.isOnlyGroup(group) });
	}

	/** A point inside this window's page. */
	function inWindow(x: number, y: number): boolean {
		return x >= 0 && y >= 0 && x < window.innerWidth && y < window.innerHeight;
	}

	/** Puts `group` beside `target`, before its pane `at`: a second view of its document when
	 * there is no other tab to go beside (the window's only tab). */
	function joinBeside(group: TabGroup, before: TabGroup | null, at: number) {
		const target = before && app.groups.includes(before) ? before : group;
		if (target === group) {
			const view = group.views[0];
			if (view && !app.showTwice(view, at === 0)) tooManyPanes();
		} else if (!app.joinTabs(target, group, at)) tooManyPanes();
	}

	/** Esc drops the tab where it is in the row, and it stays in this window; the tab on
	 * screen stays the one shown. */
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
	async function step(key: number, direction: 1 | -1) {
		const i = app.groups.findIndex((g) => g.key === key);
		app.moveTab(i, i + direction);
		await tick();
		strip?.querySelector<HTMLElement>(`[data-tab="${key}"]`)?.focus();
	}

	function onKeyDown(event: KeyboardEvent, group: TabGroup) {
		const i = app.groups.indexOf(group);
		if (event.ctrlKey && event.shiftKey && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
			event.preventDefault();
			void step(group.key, event.key === 'ArrowLeft' ? -1 : 1);
			return;
		}
		if ((event.key === 'ArrowLeft' || event.key === 'ArrowRight') && !event.altKey && !event.ctrlKey) {
			event.preventDefault();
			const n = app.groups.length;
			const next = app.groups[(i + (event.key === 'ArrowLeft' ? -1 : 1) + n) % n];
			if (next) {
				app.show(next);
				strip?.querySelector<HTMLElement>(`[data-tab="${next.key}"]`)?.focus();
			}
		} else if (event.key === 'Delete') {
			event.preventDefault();
			void app.closeGroup(group);
		} else if (isPick(event)) {
			event.preventDefault();
			app.show(group);
		}
	}

	// ----- context menu -----

	/** The tab that was right-clicked: a document's (its key), or (null) the Combine view's. */
	let contextKey = $state<number | null>(null);
	const contextGroup = $derived(contextKey === null ? null : groupOf(contextKey));
	const contextIndex = $derived(contextGroup ? app.groups.indexOf(contextGroup) : -1);
	/** The tab shown, which the right-clicked one can go beside. */
	const shownGroup = () => (app.active ? app.activeGroup : null);
	/** Lectrix's other windows, asked for as the menu opens. */
	let otherWindows = $state<WindowEntry[]>([]);

	function onContextMenu(key: number | null) {
		contextKey = key;
		otherWindows = [];
		if (key === null) return;
		void listWindows()
			.then((windows) => (otherWindows = windows))
			.catch(() => {});
	}

	/** A window as the menu names it: by what it shows, without the app's name after it. */
	function windowName(title: string): string {
		const suffix = ` - ${APP_NAME}`;
		return title.endsWith(suffix) ? title.slice(0, -suffix.length) : title || APP_NAME;
	}

	/** A tab's name for its close button and tooltip: its documents', side by side. */
	function groupName(group: TabGroup): string {
		return group.joined ? group.names.join(' and ') : (group.views[0]?.name ?? '');
	}
</script>

<svelte:window
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={endDrag}
	onblur={endDrag}
	onkeydowncapture={onDragKey}
/>

{#snippet status(view: TabGroup['views'][number])}
	{#if view.saving}
		<span
			class="size-3 shrink-0 animate-spin rounded-full border-2 border-fg-muted border-t-transparent"
			aria-label="saving"
			data-tip="Saving…"
			role="img"
			data-saving
		></span>
	{:else if view.state.dirty}
		<span
			class="size-2 shrink-0 rounded-full bg-fg"
			aria-label="unsaved changes"
			data-tip="Unsaved changes"
			role="img"
			data-unsaved
		></span>
	{/if}
{/snippet}

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
				{#each app.groups as group (group.key)}
					{@const active = group.key === app.activeKey && !app.combineActive && !app.startShown}
					{@const reachable = active || (app.startShown && group.key === app.activeKey)}
					{@const held = dragged?.id === group.key ? dragged : null}
					{@const name = groupName(group)}
					{@const first = group.views[0]}
					<div
						in:tabIn
						out:tabOut
						data-tab={group.key}
						class="doc-tab group relative flex h-8 min-w-28 shrink items-center gap-1 pr-[4px] pl-3"
						class:max-w-56={!group.joined}
						class:doc-tab-joined={group.joined}
						class:doc-tab-held={held && !held.off}
						class:opacity-50={held?.off}
						style:max-width={group.joined ? `${196 * group.views.length}px` : undefined}
						style:transform={held && !held.off ? `translateX(${held.shift}px)` : undefined}
						role="tab"
						tabindex={reachable ? 0 : -1}
						aria-selected={active}
						data-tip={group.joined ? `Side by side: ${group.names.join(', ')}` : first?.untitled ? 'Not saved yet' : first?.path}
						onpointerdown={(e) => onPointerDown(e, group)}
						onkeydown={(e) => onKeyDown(e, group)}
						oncontextmenu={() => onContextMenu(group.key)}
					>
						{#each group.views as view, i (view.key)}
							{#if i > 0}
								<span class="doc-tab-divider" aria-hidden="true"></span>
							{/if}
							{#if view.protectedNow}
								<!-- Its properties, at Security (section 6.10). -->
								<button
									type="button"
									class="tab-lock flex shrink-0 text-fg-muted"
									aria-label="Protected: see its protection"
									data-tip="Protected: click to see its protection"
									tabindex={reachable ? 0 : -1}
									data-protected
									onclick={() => openProperties(view, 'security')}
								>
									<Lock size={12} aria-hidden="true" />
								</button>
							{/if}
							<span class="min-w-0 truncate" class:text-fg-muted={!active}>{group.names[i]}</span>
							<!-- A document shown twice says its state once. -->
							{#if group.views.findIndex((v) => v.id === view.id) === i}
								{@render status(view)}
							{/if}
						{/each}
						<button
							type="button"
							data-close
							class="icon-button tab-close ml-auto shrink-0"
							aria-label="Close {name}"
							data-tip={app.hint('Close', 'closeTab')}
							tabindex={reachable ? 0 : -1}
							onclick={() => void app.closeGroup(group)}
						>
							<X size={14} aria-hidden="true" />
						</button>
					</div>
				{/each}
				{#if app.combine}
					{@const active = app.combineActive}
					{@const reachable = active || (app.startShown && app.groups.length === 0)}
					<div
						in:tabIn
						out:tabOut
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
			{#if contextKey === null}
				<ContextMenu.Item class="menu-item" onSelect={() => void app.closeCombine()}>
					Close<span class="menu-shortcut">{app.shortcut('closeTab')}</span>
				</ContextMenu.Item>
			{:else}
				{@const g = contextGroup}
				{@const key = contextKey}
				{@const shown = shownGroup()}
				{#if g && shown && shown !== g}
					<ContextMenu.Item
						class="menu-item"
						disabled={shown.views.length + g.views.length > MAX_PANES}
						onSelect={() => void app.joinTabs(shown, g)}
					>
						Show beside the current tab
					</ContextMenu.Item>
				{/if}
				{#if g && !g.joined}
					<ContextMenu.Item
						class="menu-item"
						onSelect={() => {
							const view = g.views[0];
							if (view) void app.showTwice(view);
						}}
					>
						Show twice, side by side
					</ContextMenu.Item>
				{/if}
				{#if g?.joined}
					<ContextMenu.Item class="menu-item" onSelect={() => app.separate(g)}>Separate</ContextMenu.Item>
					<ContextMenu.Separator class="menu-separator" />
					<ContextMenu.CheckboxItem
						class="menu-item"
						disabled={!!g.focused?.organize}
						checked={g.locked}
						onCheckedChange={() => toggleLock(g)}
						data-tab-sync-scrolling
					>
						{#snippet children({ checked })}
							<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>Synchronize scrolling
							{#if app.shortcut('syncScrolling')}<span class="menu-shortcut">{app.shortcut('syncScrolling')}</span>{/if}
						{/snippet}
					</ContextMenu.CheckboxItem>
					<ContextMenu.CheckboxItem
						class="menu-item"
						disabled={!!g.focused?.organize}
						checked={g.zoomLocked}
						onCheckedChange={() => toggleZoomLock(g)}
						data-tab-sync-zoom
					>
						{#snippet children({ checked })}
							<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>Synchronize zoom
							{#if app.shortcut('syncZoom')}<span class="menu-shortcut">{app.shortcut('syncZoom')}</span>{/if}
						{/snippet}
					</ContextMenu.CheckboxItem>
				{/if}
				<ContextMenu.Separator class="menu-separator" />
				<ContextMenu.Item
					class="menu-item"
					disabled={!g || app.isOnlyGroup(g)}
					onSelect={() => g && void app.moveGroupToWindow(g, { kind: 'new' })}
				>
					Move to new window
				</ContextMenu.Item>
				{#each otherWindows as other (other.label)}
					<ContextMenu.Item
						class="menu-item"
						disabled={!g}
						onSelect={() => g && void app.moveGroupToWindow(g, { kind: 'window', label: other.label })}
					>
						<span class="max-w-[320px] truncate">Move to window: {windowName(other.title)}</span>
					</ContextMenu.Item>
				{/each}
				<ContextMenu.Separator class="menu-separator" />
				<ContextMenu.Item class="menu-item" disabled={contextIndex <= 0} onSelect={() => void step(key, -1)}>
					Move left<span class="menu-shortcut">Ctrl+Shift+Left</span>
				</ContextMenu.Item>
				<ContextMenu.Item
					class="menu-item"
					disabled={contextIndex < 0 || contextIndex >= app.groups.length - 1}
					onSelect={() => void step(key, 1)}
				>
					Move right<span class="menu-shortcut">Ctrl+Shift+Right</span>
				</ContextMenu.Item>
				<ContextMenu.Separator class="menu-separator" />
				<ContextMenu.Item class="menu-item" disabled={!g} onSelect={() => g && void app.closeGroup(g)}>
					Close<span class="menu-shortcut">{app.shortcut('closeTab')}</span>
				</ContextMenu.Item>
			{/if}
		</ContextMenu.Content>
	</ContextMenu.Portal>
</ContextMenu.Root>

<!-- The tab pulled off its row, following the pointer while it is over this window. -->
{#if dragged?.off && draggedGroup}
	<div class="panel-drag-ghost" style:left="{dragged.x + 12}px" style:top="{dragged.y + 12}px" aria-hidden="true">
		<FileText size={16} />
		<span class="max-w-[240px] truncate">{groupName(draggedGroup)}</span>
	</div>
{/if}
