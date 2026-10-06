<script lang="ts">
	// Shown when no document is open: open a file, or pick a recent one. Recent files show as
	// a table of their names, when each was last opened and its size, or as a grid of their
	// first pages with the name, date and size under each (a switch beside the heading,
	// remembered in app data). A click on a row or card opens the file; the folder is in
	// the name's tooltip.
	import { FolderOpen, LayoutGrid, Layers, List, X } from '@lucide/svelte';
	import type { Component } from 'svelte';

	import fileIconUrl from '#lib/assets/lectrix-file-icon-small.svg';
	import iconUrl from '#lib/assets/lectrix-icon.svg';
	import { APP_NAME } from '#lib/config.ts';
	import { fileSize, fullDate, openedDate } from '#lib/format.ts';
	import { recentPreviewUrl, removeRecent, type RecentFile } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	interface Tool {
		id: string;
		label: string;
		icon: Component<{ size?: number; color?: string; 'aria-hidden'?: boolean | 'true' }>;
		shortcut?: string;
		run: () => void;
	}

	/** The tools, as tiles in a grid; more will come. */
	const TOOLS: readonly Tool[] = [
		{ id: 'open', label: 'Open…', icon: FolderOpen, shortcut: 'Ctrl+O', run: () => void app.open() },
		{ id: 'combine', label: 'Combine files…', icon: Layers, run: () => app.openCombine() }
	];

	/** Lucide icons are drawn on a 24-unit grid. */
	const ICON_SIZE = 20;
	const ICON_UNITS = 24;

	/**
	 * Where the tools' one gradient runs, in each icon's own units: the line of a 135deg CSS
	 * gradient over the whole grid, so the icons share it with the borders.
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
		title="Remove from the list"
		onclick={() => void remove(file.index)}
	>
		<X size={14} aria-hidden="true" />
	</button>
{/snippet}

<div class="flex h-full flex-col items-center overflow-auto bg-canvas px-4 pt-[12vh] pb-8">
	<div class="w-full max-w-[720px]">
		<div class="flex items-center gap-4">
			<!-- Decorative: the heading names the app. -->
			<img src={iconUrl} alt="" width="56" height="56" draggable="false" />
			<div>
				<h1 class="text-2xl font-semibold">{APP_NAME}</h1>
				<p class="mt-1 text-fg-muted">Open a PDF, or drop files anywhere in this window.</p>
			</div>
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
					<!-- The icon's stroke: the tools' gradient, mapped into the icon's units. -->
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
								<stop offset="0" style:stop-color="var(--lectrix-brand-from)" />
								<stop offset="1" style:stop-color="var(--lectrix-brand-to)" />
							</linearGradient>
						</defs>
					</svg>
					<tool.icon size={ICON_SIZE} color="url(#start-tool-gradient-{tool.id})" aria-hidden="true" />
					{tool.label}
					{#if tool.shortcut}<span class="start-tool-shortcut">{tool.shortcut}</span>{/if}
				</button>
			{/each}
		</div>

		{#if app.recent.length}
			<div class="mt-10 mb-2 flex items-center justify-between gap-4">
				<h2 id="recent-heading" class="recent-heading">Recent files</h2>
				<div class="flex gap-1" role="group" aria-label="Show recent files as">
					<button
						type="button"
						class="icon-button tool-button size-8"
						aria-label="List"
						title="List"
						aria-pressed={!app.recentFilesGrid}
						onclick={() => (app.recentFilesGrid = false)}
					>
						<List size={16} aria-hidden="true" />
					</button>
					<button
						type="button"
						class="icon-button tool-button size-8"
						aria-label="Grid"
						title="Grid"
						aria-pressed={app.recentFilesGrid}
						onclick={() => (app.recentFilesGrid = true)}
					>
						<LayoutGrid size={16} aria-hidden="true" />
					</button>
				</div>
			</div>

			{#if app.recentFilesGrid}
				<ul class="recent-grid" aria-labelledby="recent-heading">
					{#each app.recent as file (file.index)}
						{@const url = previewUrl(file)}
						<li class="recent-card" class:recent-missing={!file.exists}>
							<button type="button" class="recent-open" title={folderTip(file)} onclick={() => void app.openRecent(file.index)}>
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
									<span title={fullDate(file.openedAt)}>{openedDate(file.openedAt)}</span>
									<span aria-hidden="true">·</span>
									<span class="tabular-nums">{sizeText(file)}</span>
								</span>
							</button>
							{@render removeButton(file, 'recent-card-remove size-7')}
						</li>
					{/each}
				</ul>
			{:else}
				<table class="recent-table" aria-labelledby="recent-heading">
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
						{#each app.recent as file (file.index)}
							<tr class="recent-row" class:recent-missing={!file.exists}>
								<td>
									<button
										type="button"
										class="recent-open recent-open-row"
										title={folderTip(file)}
										onclick={() => void app.openRecent(file.index)}
									>
										<!-- Decorative: the name says what it is. -->
										<img src={fileIconUrl} alt="" height="20" class="h-5 w-auto shrink-0" draggable="false" />
										<span class="truncate">{file.name}</span>
									</button>
								</td>
								<td class="truncate" title={fullDate(file.openedAt)}>{openedDate(file.openedAt)}</td>
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
