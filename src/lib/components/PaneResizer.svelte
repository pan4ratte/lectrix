<script lang="ts">
	// The edge of a pane that resizes it (section 8), for the side panes and for any other
	// pane beside the pages (the Combine view's files): a window splitter, dragged with the
	// pointer or moved from the keyboard (arrows, Home, End); a double-click goes back to the
	// width the pane starts with. It lies on the pane's inner edge, so the pane is its
	// positioned parent and must not clip it.
	import { clampWidth, keyResize, type PaneLimits, type PaneSide } from './panes.ts';

	interface Props {
		/** The side of the window the pane is at: its other edge is the one that resizes it. */
		side: PaneSide;
		/** The width the pane is given. */
		width: number;
		/** The width it is shown at (`shownWidth`): its own, within its share of the window. */
		shown: number;
		limits: PaneLimits;
		/** The pane's accessible name. */
		label: string;
	}

	let { side, width = $bindable(), shown, limits, label }: Props = $props();

	let drag: { pointer: number; x: number; width: number } | null = $state(null);

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0) return;
		event.preventDefault();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		drag = { pointer: event.pointerId, x: event.clientX, width: shown };
	}

	function onPointerMove(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.pointer) return;
		const dx = event.clientX - drag.x;
		width = clampWidth(drag.width + (side === 'left' ? dx : -dx), limits);
	}

	function onPointerUp(event: PointerEvent) {
		if (drag?.pointer === event.pointerId) drag = null;
	}

	function onKeyDown(event: KeyboardEvent) {
		const next = keyResize(shown, event.key, side, limits);
		if (next === null) return;
		event.preventDefault();
		width = next;
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
	class="pane-resizer"
	class:pane-resizer-left={side === 'left'}
	class:pane-resizer-right={side === 'right'}
	class:pane-resizer-active={drag !== null}
	role="separator"
	aria-orientation="vertical"
	aria-label="Resize {label.toLowerCase()}"
	aria-valuemin={limits.min}
	aria-valuemax={limits.max}
	aria-valuenow={shown}
	tabindex="0"
	data-tip="Drag to resize; double-click to reset"
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={onPointerUp}
	onkeydown={onKeyDown}
	ondblclick={() => (width = limits.initial)}
></div>
