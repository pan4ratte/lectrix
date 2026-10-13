<script lang="ts">
	// Progress while pages are copied into a new file: combining files, extracting pages
	// (section 8: long operations show progress and can be cancelled). Stopping writes
	// nothing and leaves an existing file of that name alone.
	import { AlertDialog } from 'bits-ui';

	import { held } from '#lib/components/held.svelte.ts';
	import ProgressBar from '#lib/components/ProgressBar.svelte';
	import type { MergeProgress } from '#lib/ipc/index.ts';

	interface Props {
		/** What is going on: "Combining files". */
		heading: string;
		/** Shown once the first progress is in; null when nothing is copied. */
		progress: MergeProgress | null;
		stopping: boolean;
		onstop: () => void;
	}

	let { heading, progress, stopping, onstop }: Props = $props();

	/** What the dialog shows: the last progress, which stays while the dialog goes. */
	const last = held(() => progress);
	const shown = $derived(last.current);
	/** How many pages are copied, while that is what goes on (writing the file has no count). */
	const done = $derived(shown?.stage === 'copying' && shown.total > 0 ? shown.done : null);
	const status = $derived.by(() => {
		if (stopping) return 'Stopping…';
		if (!shown) return '';
		if (shown.stage === 'writing') return 'Writing the file…';
		return `Copying pages: ${shown.done.toLocaleString()} of ${shown.total.toLocaleString()}`;
	});
</script>

<AlertDialog.Root open={progress !== null}>
	<AlertDialog.Portal>
		<AlertDialog.Overlay class="dialog-overlay" />
		<AlertDialog.Content
			class="dialog-content w-[420px]"
			escapeKeydownBehavior="ignore"
			interactOutsideBehavior="ignore"
		>
			<AlertDialog.Title class="text-base font-semibold">{heading}</AlertDialog.Title>
			<AlertDialog.Description class="mt-2 text-sm text-fg-muted" aria-live="polite">{status}</AlertDialog.Description>
			<ProgressBar class="mt-4" label="{heading}: progress" value={done} max={shown?.total ?? 0} />
			<div class="mt-6 flex justify-end">
				<button type="button" class="button" disabled={stopping} onclick={onstop}>Stop</button>
			</div>
		</AlertDialog.Content>
	</AlertDialog.Portal>
</AlertDialog.Root>
