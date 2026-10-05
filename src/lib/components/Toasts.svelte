<script lang="ts">
	// Notifications: plain-language errors with a suggested next step (section 8), under
	// the update notice when there is one (ADR 0011).
	import { CircleAlert, Info, X } from '@lucide/svelte';

	import UpdateNotice from '#lib/features/update/UpdateNotice.svelte';
	import { app } from '#lib/stores/app.svelte.ts';
</script>

<div class="pointer-events-none fixed right-4 bottom-12 z-30 flex w-96 flex-col gap-2">
	<UpdateNotice />
	{#each app.toasts as toast (toast.id)}
		<div
			class="pointer-events-auto flex gap-3 rounded-panel border border-line p-3 shadow-[0_8px_16px_var(--color-page-shadow)]"
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
			<div class="min-w-0 flex-1">
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
	{/each}
</div>
