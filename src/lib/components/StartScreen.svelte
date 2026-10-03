<script lang="ts">
	// Shown when no document is open: open a file, or pick a recent one.
	import { FileText, FolderOpen, Layers, X } from '@lucide/svelte';

	import { APP_NAME } from '#lib/config.ts';
	import { removeRecent } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	async function remove(index: number) {
		await removeRecent(index).catch(() => {});
		await app.refreshRecent();
	}
</script>

<div class="flex h-full flex-col items-center overflow-auto bg-canvas px-4 pt-[12vh]">
	<div class="w-full max-w-xl">
		<h1 class="text-2xl font-semibold">{APP_NAME}</h1>
		<p class="mt-1 text-fg-muted">Open a PDF, or drop files anywhere in this window.</p>
		<div class="mt-6 flex gap-2">
			<button type="button" class="button button-primary gap-2" onclick={() => void app.open()}>
				<FolderOpen size={16} aria-hidden="true" />
				Open…
				<span class="ml-2 text-xs opacity-80">Ctrl+O</span>
			</button>
			<button type="button" class="button gap-2" onclick={() => app.openCombine()}>
				<Layers size={16} aria-hidden="true" />
				Combine files…
			</button>
		</div>

		{#if app.recent.length}
			<h2 class="mt-10 mb-2 text-sm font-semibold text-fg-muted">Recent</h2>
			<ul class="flex flex-col gap-0.5" aria-label="Recent files">
				{#each app.recent as file (file.index)}
					<li class="group flex items-center rounded-control hover:bg-hover">
						<button
							type="button"
							class="flex min-w-0 flex-1 items-center gap-3 border-0 bg-transparent px-3 py-2 text-left"
							onclick={() => void app.openRecent(file.index)}
						>
							<FileText size={18} aria-hidden="true" class="shrink-0 text-fg-muted" />
							<span class="min-w-0">
								<span class="block truncate" class:text-fg-muted={!file.exists}>{file.name}</span>
								<span class="block truncate text-xs text-fg-muted">
									{file.exists ? file.folder : `Not found: ${file.folder}`}
								</span>
							</span>
						</button>
						<button
							type="button"
							class="icon-button mr-1 size-7 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
							aria-label="Remove {file.name} from the list"
							title="Remove from the list"
							onclick={() => void remove(file.index)}
						>
							<X size={14} aria-hidden="true" />
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</div>
