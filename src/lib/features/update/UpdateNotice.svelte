<script lang="ts">
	// The floating update notice (ADR 0011), above the notifications in the bottom-right
	// corner. It never takes focus by itself: it is announced, and reached with Tab. A newer
	// release links to What's new for the changes it brings (section 6.6).
	import { CircleArrowDown } from '@lucide/svelte';

	import { APP_NAME } from '#lib/config.ts';
	import { changesSince } from '#lib/features/changelog/changelog.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import { downloadShare, downloadStatus, update } from './update.svelte.ts';

	const stage = $derived(update.stage);
	const share = $derived(stage.kind === 'downloading' ? downloadShare(stage.progress) : null);
	const hasNotes = $derived(
		stage.kind === 'available' && changesSince(stage.info.notes, stage.info.currentVersion).length > 0
	);
</script>

{#if stage.kind !== 'idle'}
	<section
		class="pointer-events-auto flex gap-3 rounded-panel border border-menu-line bg-menu p-3"
		aria-labelledby="update-title"
		aria-live="polite"
	>
		<span class="mt-0.5 shrink-0 text-accent">
			<CircleArrowDown size={16} aria-hidden="true" />
		</span>
		<div class="min-w-0 flex-1">
			{#if stage.kind === 'checking'}
				<h2 id="update-title" class="font-medium">Checking for updates…</h2>
				<p class="mt-1 text-sm text-fg-muted">Asking GitHub for the latest release of {APP_NAME}.</p>
				<div class="mt-3 h-1.5 overflow-hidden rounded-full bg-line" role="progressbar" aria-label="Checking for updates">
					<div class="progress-indeterminate h-full w-1/3 rounded-full bg-accent"></div>
				</div>
			{:else if stage.kind === 'available'}
				<h2 id="update-title" class="font-medium">{APP_NAME} {stage.info.version} is available</h2>
				<p class="mt-1 text-sm text-fg-muted">
					You have version {stage.info.currentVersion}.
					{#if hasNotes}
						{@const info = stage.info}
						<button
							type="button"
							class="notice-link"
							onclick={() => {
								app.whatsNewUpdate = info;
								app.whatsNewOpen = true;
							}}
						>
							See what’s new
						</button>
					{/if}
				</p>
				<div class="notice-actions mt-3">
					<button type="button" class="button button-primary" onclick={() => void update.update()}>Update</button>
					<button type="button" class="button" onclick={() => update.notNow()}>Not now</button>
					<button type="button" class="button" onclick={() => void update.dontAskAgain()}>Don’t ask again</button>
				</div>
			{:else if stage.kind === 'downloading'}
				<h2 id="update-title" class="font-medium">Downloading {APP_NAME} {stage.info.version}</h2>
				<p class="mt-1 text-sm text-fg-muted">{stage.stopping ? 'Stopping…' : downloadStatus(stage.progress)}</p>
				<div
					class="mt-3 h-1.5 overflow-hidden rounded-full bg-line"
					role="progressbar"
					aria-label="Download progress"
					aria-valuemin={0}
					aria-valuemax={100}
					aria-valuenow={share === null ? undefined : Math.round(share * 100)}
				>
					{#if share !== null}
						<div class="progress-fill h-full rounded-full bg-accent" style:width="{share * 100}%"></div>
					{:else}
						<div class="progress-indeterminate h-full w-1/3 rounded-full bg-accent"></div>
					{/if}
				</div>
				<div class="mt-3 flex gap-2">
					<button type="button" class="button" disabled={stage.stopping} onclick={() => void update.stop()}>Stop</button>
				</div>
			{:else}
				{#if stage.installed}
					<h2 id="update-title" class="font-medium">{APP_NAME} {stage.info.version} is installed</h2>
					<p class="mt-1 text-sm text-fg-muted">Restart {APP_NAME} to start using it.</p>
				{:else}
					<h2 id="update-title" class="font-medium">{APP_NAME} {stage.info.version} is ready to install</h2>
					<p class="mt-1 text-sm text-fg-muted">
						Restart {APP_NAME} to finish. If you restart later, it installs when you close {APP_NAME}.
					</p>
				{/if}
				<div class="mt-3 flex gap-2">
					<button
						type="button"
						class="button button-primary"
						disabled={stage.restarting}
						onclick={() => void update.restart()}
					>
						Restart now
					</button>
					<button type="button" class="button" disabled={stage.restarting} onclick={() => update.later()}>Later</button>
				</div>
			{/if}
		</div>
	</section>
{/if}
