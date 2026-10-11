<script lang="ts">
	// Shown when no document is open: open a file, or pick a recent one. Recent files show as
	// a table of their names, when each was last opened and its size, or as a grid of their
	// first pages with the name, date and size under each (a switch beside the heading,
	// remembered in app data; switching brings the files in with a short animation). A click
	// on a row or card opens the file; the folder is in the name's tooltip. After an update,
	// a block beside the app's name links to What's new. Also shown over open documents, from
	// the title bar's icon and name.
	import { FolderOpen, LayoutGrid, Layers, List, Sparkles, X } from '@lucide/svelte';
	import type { Component } from 'svelte';

	import fileIconUrl from '#lib/assets/lectrix-file-icon-small.svg';
	import iconUrl from '#lib/assets/lectrix-icon.svg';
	import { APP_NAME } from '#lib/config.ts';
	import { openWhatsNew } from '#lib/features/changelog/announce.ts';
	import { toolMark } from '#lib/features/annotations/toolMark.ts';
	import { fileSize, fullDate, openedDate } from '#lib/format.ts';
	import { recentPreviewUrl, removeRecent, type RecentFile } from '#lib/ipc/index.ts';
	import type { CommandName } from '#lib/commands.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	interface Tool {
		id: string;
		label: string;
		icon: Component<{ size?: number; color?: string; 'aria-hidden'?: boolean | 'true' }>;
		/** The command whose shortcut the tile shows. */
		command?: CommandName;
		run: () => void;
	}

	/** The tools, as tiles in a grid; more will come. */
	const TOOLS: readonly Tool[] = [
		{ id: 'open', label: 'Open…', icon: FolderOpen, command: 'open', run: () => void app.open() },
		{ id: 'combine', label: 'Combine files…', icon: Layers, run: () => app.openCombine() }
	];

	/** Lucide icons are drawn on a 24-unit grid. */
	const ICON_SIZE = 20;
	const ICON_UNITS = 24;

	/**
	 * Where the tools' one gradient runs, in each icon's own units: the line of a 135deg CSS
	 * gradient over the whole grid, so under the pointer an icon shares it with the border.
	 */
	interface Line {
		x1: number;
		y1: number;
		x2: number;
		y2: number;
	}

	let toolsBox = $state<{ width: number; height: number }>({ width: 0, height: 0 });
	/** Two device pixels, in CSS pixels: the tiles' border (see .start-tool in app.css). */
	let toolBorder = $state(1);
	let tileOffsets = $state<Record<string, { x: number; y: number }>>({});
	let iconLines = $state<Record<string, Line>>({});

	/** Measures the grid, and where each tile and icon sits in it, whenever it resizes. */
	function measureTools(grid: HTMLElement) {
		const measure = () => {
			const box = grid.getBoundingClientRect();
			toolsBox = { width: box.width, height: box.height };
			toolBorder = 2 / (window.devicePixelRatio || 1);
			// The gradient line of `135deg` over a box: through its centre, towards the
			// bottom right, as long as the box's diagonal projected on it.
			const length = (box.width + box.height) / Math.SQRT2;
			const [cx, cy] = [box.width / 2, box.height / 2];
			const [dx, dy] = [Math.SQRT1_2 * (length / 2), Math.SQRT1_2 * (length / 2)];
			const offsets: Record<string, { x: number; y: number }> = {};
			const lines: Record<string, Line> = {};
			for (const tile of grid.querySelectorAll<HTMLElement>('[data-tool]')) {
				const id = tile.dataset.tool ?? '';
				const t = tile.getBoundingClientRect();
				offsets[id] = { x: t.left - box.left, y: t.top - box.top };
				const icon = tile.querySelector('svg:not([data-gradient])')?.getBoundingClientRect();
				if (!icon) continue;
				const unit = icon.width / ICON_UNITS;
				const [ix, iy] = [icon.left - box.left, icon.top - box.top];
				lines[id] = {
					x1: (cx - dx - ix) / unit,
					y1: (cy - dy - iy) / unit,
					x2: (cx + dx - ix) / unit,
					y2: (cy + dy - iy) / unit
				};
			}
			tileOffsets = offsets;
			iconLines = lines;
		};
		const observer = new ResizeObserver(measure);
		observer.observe(grid);
		return () => observer.disconnect();
	}

	/** The table or the grid was picked since the start screen showed: the files come in
	 * animated (not the first time they show). */
	let switched = $state(false);
	/** How many files come in one after another; the rest come with the last of them. */
	const STAGGERED = 8;

	function showAs(grid: boolean) {
		if (app.recentFilesGrid === grid) return;
		switched = true;
		app.recentFilesGrid = grid;
	}

	/** The widest a preview shows, CSS pixels; rendered for the screen's pixel ratio. */
	const PREVIEW_WIDTH = 200;

	/** Previews that loaded, and ones that could not be made (shown as the file icon). */
	let loaded = $state<Record<string, true>>({});
	let failed = $state<Record<string, true>>({});

	async function remove(index: number) {
		await removeRecent(index).catch(() => {});
		await app.refreshRecent();
	}

	function previewUrl(file: RecentFile): string {
		const ratio = typeof devicePixelRatio === 'number' ? devicePixelRatio : 1;
		return recentPreviewUrl(file.index, file.openedAt, Math.min(1024, PREVIEW_WIDTH * ratio));
	}

	const folderTip = (file: RecentFile) => (file.exists ? file.folder : `Not found: ${file.folder}`);
	const sizeText = (file: RecentFile) => (file.size === null ? 'Not found' : fileSize(file.size));
</script>

{#snippet removeButton(file: RecentFile, className: string)}
	<button
		type="button"
		class="icon-button recent-remove {className}"
		aria-label="Remove {file.name} from the list"
		data-tip="Remove from the list"
		onclick={() => void remove(file.index)}
	>
		<X size={14} aria-hidden="true" />
	</button>
{/snippet}

<div class="flex h-full flex-col items-center overflow-auto bg-canvas px-4 pt-[12vh] pb-8">
	<div class="w-full max-w-[720px]">
		<div class="flex flex-wrap items-center gap-4">
			<!-- Decorative: the heading names the app. -->
			<img src={iconUrl} alt="" width="56" height="56" draggable="false" />
			<div>
				<h1 class="text-2xl font-semibold">{APP_NAME}</h1>
				<p class="mt-1 text-fg-muted">Open a PDF, or drop files anywhere in this window.</p>
			</div>
			{#if app.updatedTo !== null}
				<div class="start-update" role="status">
					<span class="shrink-0 text-accent"><Sparkles size={18} aria-hidden="true" /></span>
					<div class="min-w-0 flex-1">
						<p class="font-medium">{app.updatedTo ? `Updated to ${app.updatedTo}` : 'Updated'}</p>
						<button
							type="button"
							class="notice-link text-sm"
							onclick={() => {
								app.updatedTo = null;
								openWhatsNew();
							}}
						>
							See what’s new
						</button>
					</div>
					<button
						type="button"
						class="icon-button size-6 shrink-0"
						aria-label="Dismiss"
						onclick={() => (app.updatedTo = null)}
					>
						<X size={14} aria-hidden="true" />
					</button>
				</div>
			{/if}
		</div>
		<div
			class="start-tools mt-6"
			style:--tools-w={toolsBox.width}
			style:--tools-h={toolsBox.height}
			style:--tools-border="{toolBorder}px"
			{@attach measureTools}
		>
			{#each TOOLS as tool (tool.id)}
				{@const line = iconLines[tool.id]}
				<button
					type="button"
					class="start-tool"
					data-tool={tool.id}
					style:--tool-x={tileOffsets[tool.id]?.x ?? 0}
					style:--tool-y={tileOffsets[tool.id]?.y ?? 0}
					onclick={tool.run}
				>
					<!-- The icon's stroke: the tools' gradient, mapped into the icon's units (its
					     ends are the text's colour until the pointer is on the tile). -->
					<svg width="0" height="0" class="absolute" aria-hidden="true" data-gradient>
						<defs>
							<linearGradient
								id="start-tool-gradient-{tool.id}"
								gradientUnits="userSpaceOnUse"
								x1={line?.x1 ?? 0}
								y1={line?.y1 ?? 0}
								x2={line?.x2 ?? ICON_UNITS}
								y2={line?.y2 ?? ICON_UNITS}
							>
								<stop offset="0" style:stop-color="var(--tool-ink-from)" />
								<stop offset="1" style:stop-color="var(--tool-ink-to)" />
							</linearGradient>
						</defs>
					</svg>
					<tool.icon size={ICON_SIZE} color="url(#start-tool-gradient-{tool.id})" aria-hidden="true" />
					{tool.label}
					{#if tool.command && app.shortcut(tool.command)}<span class="start-tool-shortcut">{app.shortcut(tool.command)}</span>{/if}
				</button>
			{/each}
		</div>

		{#if app.recent.length}
			<div class="mt-10 mb-2 flex items-center justify-between gap-4">
				<h2 id="recent-heading" class="recent-heading">Recent files</h2>
				<!-- The mark of the view shown glides to the one picked, as the active tool's does. -->
				<div class="tool-group flex gap-1" role="group" aria-label="Show recent files as" {@attach toolMark}>
					<span class="tool-mark" aria-hidden="true" hidden></span>
					<button
						type="button"
						class="icon-button tool-button size-8"
						aria-label="List"
						data-tip="List"
						aria-pressed={!app.recentFilesGrid}
						onclick={() => showAs(false)}
					>
						<List size={16} aria-hidden="true" />
					</button>
					<button
						type="button"
						class="icon-button tool-button size-8"
						aria-label="Grid"
						data-tip="Grid"
						aria-pressed={app.recentFilesGrid}
						onclick={() => showAs(true)}
					>
						<LayoutGrid size={16} aria-hidden="true" />
					</button>
				</div>
			</div>

			{#if app.recentFilesGrid}
				<ul class="recent-grid" class:recent-enter={switched} aria-labelledby="recent-heading">
					{#each app.recent as file, i (file.index)}
						{@const url = previewUrl(file)}
						<li class="recent-card" class:recent-missing={!file.exists} style:--recent-i={Math.min(i, STAGGERED)}>
							<button type="button" class="recent-open" data-tip={folderTip(file)} onclick={() => void app.openRecent(file.index)}>
								<span class="recent-preview">
									{#if file.exists && !failed[url]}
										<img
											src={url}
											alt=""
											class="recent-page"
											class:recent-page-loaded={loaded[url]}
											draggable="false"
											decoding="async"
											onload={() => (loaded[url] = true)}
											onerror={() => (failed[url] = true)}
										/>
									{/if}
									{#if !file.exists || failed[url]}
										<!-- Decorative: the name says what it is. -->
										<img src={fileIconUrl} alt="" class="h-12 w-auto" draggable="false" />
									{/if}
								</span>
								<span class="recent-name">{file.name}</span>
								<span class="recent-details">
									<span data-tip={fullDate(file.openedAt)}>{openedDate(file.openedAt)}</span>
									<span aria-hidden="true">·</span>
									<span class="tabular-nums">{sizeText(file)}</span>
								</span>
							</button>
							{@render removeButton(file, 'recent-card-remove size-7')}
						</li>
					{/each}
				</ul>
			{:else}
				<table class="recent-table" class:recent-enter={switched} aria-labelledby="recent-heading">
					<colgroup>
						<col />
						<col class="w-[168px]" />
						<col class="w-[104px]" />
						<col class="w-[40px]" />
					</colgroup>
					<thead>
						<tr>
							<th scope="col">Name</th>
							<th scope="col">Opened</th>
							<th scope="col" class="text-right">Size</th>
							<th scope="col"><span class="sr-only">Remove</span></th>
						</tr>
					</thead>
					<tbody>
						{#each app.recent as file, i (file.index)}
							<tr class="recent-row" class:recent-missing={!file.exists} style:--recent-i={Math.min(i, STAGGERED)}>
								<td>
									<button
										type="button"
										class="recent-open recent-open-row"
										data-tip={folderTip(file)}
										onclick={() => void app.openRecent(file.index)}
									>
										<!-- Decorative: the name says what it is. -->
										<img src={fileIconUrl} alt="" height="20" class="h-5 w-auto shrink-0" draggable="false" />
										<span class="truncate">{file.name}</span>
									</button>
								</td>
								<td class="truncate" data-tip={fullDate(file.openedAt)}>{openedDate(file.openedAt)}</td>
								<td class="text-right tabular-nums">{sizeText(file)}</td>
								<td>{@render removeButton(file, 'size-7')}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		{/if}
	</div>
</div>
