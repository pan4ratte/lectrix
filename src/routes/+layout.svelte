<script lang="ts">
	import '../app.css';

	import Tooltips from '#lib/components/Tooltips.svelte';

	let { children } = $props();
</script>

{@render children()}
<Tooltips />
