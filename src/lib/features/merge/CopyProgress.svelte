<script lang="ts">
	// Progress while pages are copied into a new file: combining files, extracting pages
	// (section 8: long operations show progress and can be cancelled). Stopping writes
	// nothing and leaves an existing file of that name alone.
	import { AlertDialog } from 'bits-ui';

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

	const copying = $derived(progress?.stage === 'copying');
	const share = $derived(progress && copying && progress.total > 0 ? progress.done / progress.total : null);
	const status = $derived.by(() => {
		if (stopping) return 'Stopping…';
		if (!progress) return '';
		if (progress.stage === 'writing') return 'Writing the file…';
		return `Copying pages: ${progress.done.toLocaleString()} of ${progress.total.toLocaleString()}`;
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
			<div
				class="mt-4 h-1.5 overflow-hidden rounded-full bg-line"
				role="progressbar"
				aria-label="{heading}: progress"
				aria-valuemin={0}
				aria-valuemax={progress?.total ?? 0}
				aria-valuenow={copying ? progress?.done : undefined}
			>
				{#if share !== null}
					<div class="h-full rounded-full bg-accent" style:width="{Math.round(share * 100)}%"></div>
				{:else}
					<div class="progress-indeterminate h-full w-1/3 rounded-full bg-accent"></div>
				{/if}
			</div>
			<div class="mt-6 flex justify-end">
				<button type="button" class="button" disabled={stopping} onclick={onstop}>Stop</button>
			</div>
		</AlertDialog.Content>
	</AlertDialog.Portal>
</AlertDialog.Root>
