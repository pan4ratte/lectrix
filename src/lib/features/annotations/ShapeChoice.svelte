<script lang="ts">
	// What the Shapes tool draws (section 6.5), the first block of its panel: line, arrow,
	// rectangle, ellipse, each with what Shift does while it is drawn. A radio group: one Tab
	// stop, and the arrow keys move among the shapes and pick the one they reach.
	import { Check } from '@lucide/svelte';

	import { SHAPES, type ShapeKind } from './pen.ts';

	let { shape, onshape }: { shape: ShapeKind; onshape: (shape: ShapeKind) => void } = $props();

	const uid = $props.id();
	let group = $state<HTMLElement | null>(null);

	function onShapeKey(event: KeyboardEvent, index: number) {
		const last = SHAPES.length - 1;
		const steps: Record<string, number> = {
			ArrowDown: index === last ? 0 : index + 1,
			ArrowRight: index === last ? 0 : index + 1,
			ArrowUp: index === 0 ? last : index - 1,
			ArrowLeft: index === 0 ? last : index - 1,
			Home: 0,
			End: last
		};
		const next = steps[event.key];
		if (next === undefined) return;
		event.preventDefault();
		onshape(SHAPES[next]!.id);
		group?.querySelectorAll<HTMLElement>('.shape-choice')[next]?.focus();
	}
</script>

<span id="{uid}-label" class="adjust-value mb-1">Shape</span>
<div bind:this={group} class="flex flex-col" role="radiogroup" aria-labelledby="{uid}-label">
	{#each SHAPES as s, i (s.id)}
		<button
			type="button"
			class="shape-choice"
			role="radio"
			aria-checked={s.id === shape}
			tabindex={s.id === shape ? 0 : -1}
			onclick={() => onshape(s.id)}
			onkeydown={(e) => onShapeKey(e, i)}
		>
			<span class="w-4" aria-hidden="true">
				{#if s.id === shape}<Check size={14} />{/if}
			</span>
			<s.icon size={16} aria-hidden="true" />
			<span class="flex-1 text-left">{s.label}</span>
			<span class="menu-shortcut" aria-hidden="true">Shift: {s.shift}</span>
		</button>
	{/each}
</div>
<div class="tool-options-rule" aria-hidden="true"></div>
