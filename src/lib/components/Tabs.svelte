<script lang="ts">
	// Document tabs: click to switch, drag (or Ctrl+Shift+Left/Right) to reorder, middle-click
	// or the close button to close. A dot marks unsaved changes.
	import { X } from '@lucide/svelte';

	import { app } from '#lib/stores/app.svelte.ts';

	let strip: HTMLDivElement | undefined = $state();
	let drag: { id: number; startX: number; moved: boolean } | null = null;

	function onPointerDown(event: PointerEvent, id: number) {
		if (event.button === 1) {
			event.preventDefault();
			void app.closeTab(id);
			return;
		}
		if (event.button !== 0) return;
		if ((event.target as HTMLElement).closest('[data-close]')) return;
		app.activate(id);
		drag = { id, startX: event.clientX, moved: false };
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
	}

	function onPointerMove(event: PointerEvent) {
		if (!drag || !strip) return;
		if (!drag.moved && Math.abs(event.clientX - drag.startX) < 6) return;
		drag.moved = true;
		const tabs = [...strip.querySelectorAll<HTMLElement>('[data-tab]')];
		const from = app.tabs.findIndex((t) => t.id === drag!.id);
		let to = tabs.findIndex((el) => {
			const r = el.getBoundingClientRect();
			return event.clientX < r.left + r.width / 2;
		});
		if (to < 0) to = tabs.length - 1;
		else if (to > from) to--;
		if (to !== from) app.moveTab(from, to);
	}

	function onPointerUp() {
		drag = null;
	}

	function onKeyDown(event: KeyboardEvent, id: number) {
		const i = app.tabs.findIndex((t) => t.id === id);
		if (event.ctrlKey && event.shiftKey && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
			event.preventDefault();
			app.moveTab(i, i + (event.key === 'ArrowLeft' ? -1 : 1));
			return;
		}
		if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
			event.preventDefault();
			const next = app.tabs[(i + (event.key === 'ArrowLeft' ? -1 : 1) + app.tabs.length) % app.tabs.length];
			if (next) {
				app.activate(next.id);
				strip?.querySelector<HTMLElement>(`[data-tab="${next.id}"]`)?.focus();
			}
		} else if (event.key === 'Delete') {
			event.preventDefault();
			void app.closeTab(id);
		}
	}
</script>

<div bind:this={strip} class="flex h-full min-w-0 items-end gap-0.5" role="tablist" aria-label="Open documents">
	{#each app.tabs as tab (tab.id)}
		{@const active = tab.id === app.activeId}
		<div
			data-tab={tab.id}
			class="group relative flex h-8 max-w-56 min-w-28 shrink items-center gap-1 rounded-t-panel pr-1 pl-3"
			class:bg-surface={active}
			class:hover:bg-hover={!active}
			role="tab"
			tabindex={active ? 0 : -1}
			aria-selected={active}
			title={tab.path}
			onpointerdown={(e) => onPointerDown(e, tab.id)}
			onpointermove={onPointerMove}
			onpointerup={onPointerUp}
			onkeydown={(e) => onKeyDown(e, tab.id)}
		>
			<span class="truncate" class:text-fg-muted={!active}>{tab.name}</span>
			{#if tab.state.dirty}
				<span class="size-2 shrink-0 rounded-full bg-fg" aria-label="unsaved changes" role="img"></span>
			{/if}
			<button
				type="button"
				data-close
				class="icon-button ml-auto size-6 shrink-0"
				aria-label="Close {tab.name}"
				title="Close (Ctrl+W)"
				tabindex={active ? 0 : -1}
				onclick={() => void app.closeTab(tab.id)}
			>
				<X size={14} aria-hidden="true" />
			</button>
		</div>
	{/each}
</div>
