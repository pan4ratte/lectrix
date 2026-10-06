<script lang="ts">
	// The custom colour in the colour panel (section 6.5): a square for saturation and
	// brightness, a hue bar, the eyedropper where the webview has one, and a field that takes
	// a hex value (six or three digits; no RGB or HSL). Dragging and typing preview in the
	// swatch beside the field. The colour applies when the pointer is let go, on Enter or
	// leaving the field, half a second after the arrow keys stop, or when the eyedropper
	// picks one, so each choice is one undo step.
	import { Pipette } from '@lucide/svelte';
	import { onDestroy, untrack } from 'svelte';

	import { hexToHsv, hsvToHex, parseHex, type Hsv } from './color.ts';

	interface Props {
		/** The colour in use (#rrggbb), or null for none. */
		color: string | null;
		onpick: (color: string) => void;
	}

	let { color, onpick }: Props = $props();
	const uid = $props.id();
	const hintId = `${uid}-hint`;

	/** What the picker starts from when the annotation has no colour. */
	const START = '#e52237';
	/** How long after the last arrow key the colour applies. */
	const KEY_SETTLE_MS = 500;

	interface EyeDropperApi {
		open(): Promise<{ sRGBHex: string }>;
	}
	const EyeDropper = (globalThis as { EyeDropper?: new () => EyeDropperApi }).EyeDropper;

	let hsv = $state<Hsv>(hexToHsv(untrack(() => color) ?? START));
	/** The field's text while it is being typed in; null shows the colour. */
	let typed = $state<string | null>(null);
	let square = $state<HTMLElement | null>(null);
	let hueBar = $state<HTMLElement | null>(null);
	let dragging: 'square' | 'hue' | null = null;
	let settle: ReturnType<typeof setTimeout> | undefined;

	const draft = $derived(hsvToHex(hsv));
	const shown = $derived(typed ?? draft.slice(1).toUpperCase());
	const invalid = $derived(typed !== null && parseHex(typed) === null);

	/** Takes a colour, keeping the hue when it is a grey (which has none of its own). */
	function take(hex: string) {
		const next = hexToHsv(hex);
		hsv = next.s === 0 || next.v === 0 ? { ...next, h: hsv.h } : next;
	}

	// Follows the colour in use when it changes elsewhere (a preset, undo).
	$effect(() => {
		const current = color;
		untrack(() => {
			if (current === null || dragging || settle !== undefined || current === draft) return;
			typed = null;
			take(current);
		});
	});

	function commit() {
		clearTimeout(settle);
		settle = undefined;
		typed = null;
		if (draft !== color) onpick(draft);
	}

	function commitSoon() {
		clearTimeout(settle);
		settle = setTimeout(commit, KEY_SETTLE_MS);
	}

	/** Applies keyboard changes still waiting when the control is left or the panel closes. */
	function flush() {
		if (settle !== undefined) commit();
	}
	onDestroy(flush);

	const clamp = (x: number) => Math.min(1, Math.max(0, x));

	function moveTo(event: PointerEvent) {
		if (dragging === 'square' && square) {
			const r = square.getBoundingClientRect();
			hsv = { ...hsv, s: clamp((event.clientX - r.left) / r.width), v: clamp(1 - (event.clientY - r.top) / r.height) };
		} else if (dragging === 'hue' && hueBar) {
			const r = hueBar.getBoundingClientRect();
			hsv = { ...hsv, h: clamp((event.clientX - r.left) / r.width) * 359 };
		}
	}

	function onPointerDown(event: PointerEvent, what: 'square' | 'hue') {
		if (event.button !== 0) return;
		event.preventDefault();
		const target = event.currentTarget as HTMLElement;
		target.setPointerCapture(event.pointerId);
		target.focus({ preventScroll: true });
		clearTimeout(settle);
		settle = undefined;
		typed = null;
		dragging = what;
		moveTo(event);
	}

	function onPointerUp() {
		if (!dragging) return;
		dragging = null;
		commit();
	}

	/** Arrows move by 1% (hue: 1°), with Shift by 10%; Enter applies at once. */
	function onSquareKey(event: KeyboardEvent) {
		const step = event.shiftKey ? 0.1 : 0.01;
		const moves: Record<string, [number, number]> = {
			ArrowLeft: [-step, 0],
			ArrowRight: [step, 0],
			ArrowUp: [0, step],
			ArrowDown: [0, -step]
		};
		const move = moves[event.key];
		if (move) {
			hsv = { ...hsv, s: clamp(hsv.s + move[0]), v: clamp(hsv.v + move[1]) };
			typed = null;
			commitSoon();
		} else if (event.key === 'Enter') commit();
		else return;
		event.preventDefault();
		event.stopPropagation();
	}

	function onHueKey(event: KeyboardEvent) {
		const step = event.shiftKey ? 10 : 1;
		const to: Record<string, number> = {
			ArrowLeft: hsv.h - step,
			ArrowDown: hsv.h - step,
			ArrowRight: hsv.h + step,
			ArrowUp: hsv.h + step,
			Home: 0,
			End: 359
		};
		const h = to[event.key];
		if (h !== undefined) {
			hsv = { ...hsv, h: Math.min(359, Math.max(0, h)) };
			typed = null;
			commitSoon();
		} else if (event.key === 'Enter') commit();
		else return;
		event.preventDefault();
		event.stopPropagation();
	}

	function onInput(event: Event) {
		typed = (event.currentTarget as HTMLInputElement).value;
		const hex = parseHex(typed);
		if (hex) take(hex);
	}

	function onFieldKey(event: KeyboardEvent) {
		if (event.key !== 'Enter') return;
		event.preventDefault();
		event.stopPropagation();
		if (!invalid) commit();
	}

	/** Leaving the field applies a valid value and puts the colour in use back otherwise. */
	function onFieldBlur() {
		if (typed === null) return;
		if (invalid) {
			typed = null;
			if (color) take(color);
		} else commit();
	}

	async function sample() {
		if (!EyeDropper) return;
		try {
			const hex = parseHex((await new EyeDropper().open()).sRGBHex);
			if (!hex) return;
			typed = null;
			take(hex);
			commit();
		} catch {
			// Cancelled with Esc.
		}
	}
</script>

<div class="mt-2 flex flex-col gap-2 border-t border-menu-line pt-2">
	<div
		bind:this={square}
		class="hue-square"
		style:--hue={hsv.h}
		role="slider"
		tabindex="0"
		aria-label="Saturation and brightness"
		aria-valuemin={0}
		aria-valuemax={100}
		aria-valuenow={Math.round(hsv.s * 100)}
		aria-valuetext="Saturation {Math.round(hsv.s * 100)}%, brightness {Math.round(hsv.v * 100)}%"
		onpointerdown={(e) => onPointerDown(e, 'square')}
		onpointermove={moveTo}
		onpointerup={onPointerUp}
		onpointercancel={onPointerUp}
		onkeydown={onSquareKey}
		onblur={flush}
	>
		<span class="picker-handle" style:left="{hsv.s * 100}%" style:top="{(1 - hsv.v) * 100}%" style:--swatch={draft}></span>
	</div>
	<div
		bind:this={hueBar}
		class="hue-bar"
		role="slider"
		tabindex="0"
		aria-label="Hue"
		aria-valuemin={0}
		aria-valuemax={359}
		aria-valuenow={Math.round(hsv.h)}
		aria-valuetext="{Math.round(hsv.h)}°"
		onpointerdown={(e) => onPointerDown(e, 'hue')}
		onpointermove={moveTo}
		onpointerup={onPointerUp}
		onpointercancel={onPointerUp}
		onkeydown={onHueKey}
		onblur={flush}
	>
		<span class="picker-handle" style:left="{(hsv.h / 359) * 100}%" style:top="50%" style:--swatch="hsl({hsv.h} 100% 50%)"></span>
	</div>
	<div class="flex items-center gap-2">
		{#if EyeDropper}
			<button type="button" class="icon-button size-7 shrink-0" aria-label="Pick a colour from the screen" title="Pick a colour from the screen" onclick={() => void sample()}>
				<Pipette size={16} aria-hidden="true" />
			</button>
		{/if}
		<span class="swatch-dot shrink-0" style:--swatch={draft} aria-hidden="true"></span>
		<label class="hex-field field" class:hex-field-invalid={invalid}>
			<span class="text-fg-muted" aria-hidden="true">#</span>
			<input
				class="min-w-0 flex-1 bg-transparent uppercase outline-none"
				value={shown}
				maxlength={7}
				spellcheck="false"
				autocomplete="off"
				aria-label="Hex colour"
				aria-invalid={invalid}
				aria-describedby={invalid ? hintId : undefined}
				oninput={onInput}
				onkeydown={onFieldKey}
				onblur={onFieldBlur}
			/>
		</label>
	</div>
	{#if invalid}
		<p id={hintId} class="text-xs text-danger">Type six hex digits, such as E52237.</p>
	{/if}
</div>
