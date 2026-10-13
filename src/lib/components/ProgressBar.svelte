<script lang="ts">
	// A progress bar (section 8), for every one in the app: copying pages into a new file,
	// checking for an update, downloading one. With a value, its filled part glides to each
	// new one and a sheen runs along it, so the bar shows that work goes on between two
	// values too; without one (how far is not known), a part of it runs from end to end.
	// Reduced motion stills both.
	interface Props {
		/** How far, from 0 to `max`; null while that is not known. */
		value: number | null;
		max?: number;
		/** The bar's accessible name. */
		label: string;
		class?: string;
	}

	let { value, max = 100, label, class: className = '' }: Props = $props();

	const share = $derived(value === null || max <= 0 ? null : Math.max(0, Math.min(1, value / max)));
</script>

<div
	class={['progress', className]}
	role="progressbar"
	aria-label={label}
	aria-valuemin={0}
	aria-valuemax={max}
	aria-valuenow={share === null || value === null ? undefined : Math.round(value)}
>
	{#if share !== null}
		<div class="progress-fill" style:width="{share * 100}%"></div>
	{:else}
		<div class="progress-indeterminate"></div>
	{/if}
</div>
