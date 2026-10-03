<script lang="ts">
	// The app's menus, in the custom title bar.
	import { Menubar } from 'bits-ui';

	import { commands } from '#lib/commands.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	const tab = $derived(app.active);
	const hasDoc = $derived(tab !== null);
</script>

<Menubar.Root class="flex items-center gap-0.5" aria-label="Main menu">
	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">File</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={4}>
				<Menubar.Item class="menu-item" onSelect={commands.open}>
					Open…<span class="menu-shortcut">Ctrl+O</span>
				</Menubar.Item>
				<Menubar.Sub>
					<Menubar.SubTrigger class="menu-item" disabled={app.recent.length === 0}>
						Open recent
					</Menubar.SubTrigger>
					<Menubar.SubContent class="menu-content max-w-[480px]">
						{#each app.recent as file (file.index)}
							<Menubar.Item class="menu-item" onSelect={() => void app.openRecent(file.index)}>
								<span class="truncate" class:text-fg-muted={!file.exists} title="{file.folder}\{file.name}">
									{file.name}
								</span>
							</Menubar.Item>
						{/each}
					</Menubar.SubContent>
				</Menubar.Sub>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.save}>
					Save<span class="menu-shortcut">Ctrl+S</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.saveAs}>
					Save as…<span class="menu-shortcut">Ctrl+Shift+S</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.saveAsOptimized}>
					Save as (optimized)…
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.reload}>
					Reload from disk
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.closeTab}>
					Close<span class="menu-shortcut">Ctrl+W</span>
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" onSelect={commands.exit}>Exit</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>

	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">Edit</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={4}>
				<Menubar.Item class="menu-item" disabled={!tab?.state.undoName} onSelect={commands.undo}>
					{tab?.state.undoName ? `Undo ${tab.state.undoName.toLowerCase()}` : 'Undo'}
					<span class="menu-shortcut">Ctrl+Z</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!tab?.state.redoName} onSelect={commands.redo}>
					{tab?.state.redoName ? `Redo ${tab.state.redoName.toLowerCase()}` : 'Redo'}
					<span class="menu-shortcut">Ctrl+Y</span>
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!tab?.selection} onSelect={commands.copy}>
					Copy<span class="menu-shortcut">Ctrl+C</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.find}>
					Find…<span class="menu-shortcut">Ctrl+F</span>
				</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>

	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">View</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={4}>
				<Menubar.CheckboxItem
					class="menu-item"
					checked={app.sidebarOpen}
					onCheckedChange={commands.toggleSidebar}
				>
					{#snippet children({ checked })}
						<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>Sidebar
					{/snippet}
				</Menubar.CheckboxItem>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.showBookmarks}>
					Bookmarks
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.showLabels}>
					Page labels
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.zoomIn}>
					Zoom in<span class="menu-shortcut">Ctrl+=</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.zoomOut}>
					Zoom out<span class="menu-shortcut">Ctrl+-</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.fitWidth}>
					Fit width<span class="menu-shortcut">Ctrl+0</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.fitPage}>
					Fit page
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.rotateViewClockwise}>
					Rotate view clockwise
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.rotateViewCounterClockwise}>
					Rotate view counter-clockwise
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!hasDoc} onSelect={commands.goToPage}>
					Go to page…<span class="menu-shortcut">Ctrl+G</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!tab?.history.canGoBack} onSelect={commands.back}>
					Back<span class="menu-shortcut">Alt+Left</span>
				</Menubar.Item>
				<Menubar.Item class="menu-item" disabled={!tab?.history.canGoForward} onSelect={commands.forward}>
					Forward<span class="menu-shortcut">Alt+Right</span>
				</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>

	<Menubar.Menu>
		<Menubar.Trigger class="menubar-trigger">Document</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content class="menu-content" align="start" sideOffset={4}>
				<Menubar.Item
					class="menu-item"
					disabled={!hasDoc || !tab?.flags.canAssemble}
					onSelect={commands.rotatePages}
				>
					Rotate pages…
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!tab?.canEditBookmarks} onSelect={commands.addBookmark}>
					Add bookmark<span class="menu-shortcut">Ctrl+B</span>
				</Menubar.Item>
				<Menubar.Separator class="menu-separator" />
				<Menubar.Item class="menu-item" disabled={!tab?.canEditLabels} onSelect={commands.startLabelRange}>
					New label range from this page
				</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>
</Menubar.Root>
