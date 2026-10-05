<script lang="ts">
	// Status bar: the save state. The page box and zoom are in the view bar at the top.
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	let { tab }: { tab: DocTab } = $props();

	const saveState = $derived(
		tab.saving ? 'Saving…' : tab.state.dirty ? 'Unsaved changes' : 'All changes saved'
	);
</script>

<footer
	class="flex h-8 shrink-0 items-center gap-3 border-t border-line bg-chrome px-3 text-sm text-fg-muted"
>
	<span class="ml-auto min-w-32 text-right" aria-live="polite">{saveState}</span>
</footer>
