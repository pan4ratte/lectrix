<script lang="ts">
	// The app's menus, in the custom title bar (Windows and Linux; macOS has them in the
	// system's menu bar, NativeMenus.svelte), as menus.svelte.ts lays them out.
	import { ChevronRight } from '@lucide/svelte';
	import { Menubar } from 'bits-ui';

	import type { CommandName } from '#lib/commands.ts';
	import { appMenus, type MenuEntry } from '#lib/menus.svelte.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	const menus = $derived(appMenus());

	/** A submenu's first item level with the item that opens it: up by the menu's padding
	 * (4 px) and edge (1 px). */
	const SUBMENU_OFFSET = -5;
</script>

<!-- A command's shortcut, as Settings has it. -->
{#snippet keys(command: CommandName | undefined)}
	{@const text = command ? app.shortcut(command) : ''}
	{#if text}<span class="menu-shortcut">{text}</span>{/if}
{/snippet}

{#snippet chevron()}
	<ChevronRight size={14} class="ml-auto text-fg-muted" aria-hidden="true" />
{/snippet}

{#snippet check(checked: boolean)}
	<span class="w-4" aria-hidden="true">{checked ? '✓' : ''}</span>
{/snippet}

{#snippet entries(items: readonly MenuEntry[])}
	{#each items as entry, i (entry.kind === 'item' || entry.kind === 'radio' ? entry.id : `${entry.kind}-${i}`)}
		{#if entry.kind === 'item' && entry.checked !== undefined}
			<Menubar.CheckboxItem
				class="menu-item"
				disabled={entry.enabled === false}
				checked={entry.checked}
				onCheckedChange={() => entry.run()}
			>
				{#snippet children({ checked })}
					{@render check(checked)}{entry.label}{@render keys(entry.command)}
				{/snippet}
			</Menubar.CheckboxItem>
		{:else if entry.kind === 'item'}
			<Menubar.Item class="menu-item" disabled={entry.enabled === false} onSelect={() => entry.run()}>
				{#if entry.tip !== undefined || entry.muted}
					<span class="truncate" class:text-fg-muted={entry.muted} data-tip={entry.tip}>{entry.label}</span>
				{:else}
					{entry.label}
				{/if}
				{@render keys(entry.command)}
			</Menubar.Item>
		{:else if entry.kind === 'radio'}
			<Menubar.RadioGroup value={entry.value} onValueChange={(v) => entry.choose(v)}>
				{#each entry.options as option (option.value)}
					<Menubar.RadioItem class="menu-item" value={option.value}>
						{#snippet children({ checked })}
							{@render check(checked)}{option.label}
						{/snippet}
					</Menubar.RadioItem>
				{/each}
			</Menubar.RadioGroup>
		{:else if entry.kind === 'separator'}
			<Menubar.Separator class="menu-separator" />
		{:else if entry.kind === 'submenu'}
			<Menubar.Sub>
				<Menubar.SubTrigger class="menu-item" disabled={entry.enabled === false}>{entry.label}{@render chevron()}</Menubar.SubTrigger>
				<Menubar.SubContent class={['menu-content', entry.wide && 'max-w-[480px]']} align="start" alignOffset={SUBMENU_OFFSET}>
					{@render entries(entry.items)}
				</Menubar.SubContent>
			</Menubar.Sub>
		{/if}
	{/each}
{/snippet}

<!-- The menus open from the title bar's bottom edge, as in VS Code: the 22 px triggers sit about 7 px
     above it in the 35 px bar. -->
<Menubar.Root class="flex items-center" aria-label="Main menu">
	{#each menus as menu (menu.label)}
		<Menubar.Menu>
			<Menubar.Trigger class="menubar-trigger">{menu.label}</Menubar.Trigger>
			<Menubar.Portal>
				<Menubar.Content class="menu-content" align="start" sideOffset={7}>
					{@render entries(menu.items)}
				</Menubar.Content>
			</Menubar.Portal>
		</Menubar.Menu>
	{/each}
</Menubar.Root>
