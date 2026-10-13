<script lang="ts">
	// Notifications: plain-language errors with a suggested next step (section 8), under
	// the update notice when there is one (ADR 0011). Each has a place in the column, which
	// opens as it comes and closes as it goes (`noticeMotion`), so the others make way.
	import { CircleAlert, Info, X } from '@lucide/svelte';

	import UpdateNotice from '#lib/features/update/UpdateNotice.svelte';
	import { app } from '#lib/stores/app.svelte.ts';

	import { noticeMotion } from './notices.ts';
</script>

<div class="pointer-events-none fixed right-4 bottom-12 z-30 flex w-96 flex-col">
	<UpdateNotice />
	{#each app.toasts as toast (toast.id)}
		<div class="notice-slot" transition:noticeMotion>
			<div
				class="notice pointer-events-auto flex gap-3 rounded-panel border border-menu-line p-3"
				class:bg-danger-bg={toast.kind === 'error'}
				class:bg-info-bg={toast.kind === 'info'}
				role={toast.kind === 'error' ? 'alert' : 'status'}
			>
				<span class="mt-0.5 shrink-0" class:text-danger={toast.kind === 'error'}>
					{#if toast.kind === 'error'}
						<CircleAlert size={16} aria-hidden="true" />
					{:else}
						<Info size={16} aria-hidden="true" />
					{/if}
				</span>
				<!-- A file name or an address without spaces breaks where the line ends. -->
				<div class="min-w-0 flex-1 wrap-anywhere">
					<p class="font-medium">{toast.message}</p>
					{#if toast.suggestion}
						<p class="mt-1 text-sm text-fg-muted">{toast.suggestion}</p>
					{/if}
					{#if toast.action}
						<button
							type="button"
							class="button mt-2"
							onclick={() => {
								toast.action?.run();
								app.dismissToast(toast.id);
							}}
						>
							{toast.action.label}
						</button>
					{/if}
				</div>
				<button
					type="button"
					class="icon-button size-6 shrink-0"
					aria-label="Dismiss"
					onclick={() => app.dismissToast(toast.id)}
				>
					<X size={14} aria-hidden="true" />
				</button>
			</div>
		</div>
	{/each}
</div>
