<script lang="ts">
	// Settings > Shortcuts (section 6.6): every command that can have a keyboard shortcut,
	// in groups, with a search field that filters them, then the keys with a fixed use. A
	// shortcut is a chip whose key is a button: pressing it, then the new keys, changes it;
	// Backspace takes it away and Esc leaves it. The plus at the end of a row adds one (a
	// command can have several), and a shortcut that was added has an x that removes it.
	import { Plus, Search, X } from '@lucide/svelte';
	import { tick } from 'svelte';

	import {
		FIXED_KEYS,
		MAX_KEYS,
		SHORTCUT_GROUPS,
		chordOf,
		commandUsing,
		defaultKeys,
		displayShortcut,
		keysOf,
		reservedReason,
		withKeys,
		type ShortcutCommand
	} from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import type { Draft } from './schema.ts';

	interface Props {
		draft: Draft;
		commit: () => void;
		/** Keys are being taken for a shortcut: the dialog leaves Esc to this panel. */
		capturing?: boolean;
	}

	let { draft, commit, capturing = $bindable(false) }: Props = $props();

	let root = $state<HTMLElement | null>(null);
	let query = $state('');
	/** The shortcut being changed: a command and which of its keys (one past the last adds one). */
	let recording = $state<{ command: string; slot: number } | null>(null);
	/** Why the keys just pressed were not taken, under the row they were pressed for. */
	let problem = $state('');

	$effect(() => {
		capturing = recording !== null;
	});

	const words = $derived(query.toLowerCase().split(/ +/).filter(Boolean));
	const matches = (text: string) => words.every((w) => text.toLowerCase().includes(w));

	const groups = $derived(
		SHORTCUT_GROUPS.map((g) => ({
			title: g.title,
			commands: g.commands.filter((c) => {
				const keys = keysOf(c.command, draft.shortcuts);
				return matches(`${g.title} ${c.label} ${keys.join(' ')} ${keys.map((k) => displayShortcut(k)).join(' ')}`);
			})
		})).filter((g) => g.commands.length > 0)
	);
	const fixed = $derived(FIXED_KEYS.filter((f) => matches(`fixed keys ${f.label} ${f.keys} ${displayShortcut(f.keys)}`)));
	const changed = $derived(Object.keys(draft.shortcuts).length > 0);

	function set(c: ShortcutCommand, keys: readonly string[]) {
		draft.shortcuts = withKeys(draft.shortcuts, c.command, keys);
		commit();
	}

	/** Puts the focus on a command's shortcut (the last one, past the end; the plus button,
	 * without any), once it is drawn. */
	async function focusSlot(command: string, slot: number) {
		await tick();
		const row = root?.querySelector(`[data-command="${command}"]`);
		const slots = row?.querySelectorAll<HTMLElement>('[data-slot]');
		const target = slots?.[Math.min(slot, slots.length - 1)] ?? row?.querySelector<HTMLElement>('[data-add]');
		target?.focus();
	}

	function record(c: ShortcutCommand, slot: number) {
		problem = '';
		recording = { command: c.command, slot };
		void focusSlot(c.command, slot);
	}

	function stop() {
		recording = null;
		problem = '';
	}

	function onKey(event: KeyboardEvent, c: ShortcutCommand, slot: number) {
		if (recording?.command !== c.command || recording.slot !== slot) return;
		// Tab moves on, which leaves the shortcut as it was.
		if (event.key === 'Tab') return;
		event.preventDefault();
		event.stopPropagation();
		const keys = keysOf(c.command, draft.shortcuts);
		const plain = !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
		if (plain && event.key === 'Escape') {
			stop();
			void focusSlot(c.command, slot);
			return;
		}
		if (plain && (event.key === 'Backspace' || event.key === 'Delete')) {
			stop();
			if (slot < keys.length) set(c, keys.filter((_, i) => i !== slot));
			void focusSlot(c.command, slot);
			return;
		}
		const pressed = chordOf(event);
		if (!pressed) return;
		const reserved = reservedReason(pressed);
		if (reserved) {
			problem = reserved;
			return;
		}
		const other = commandUsing(pressed, draft.shortcuts, c.command);
		if (other) {
			problem = `${displayShortcut(pressed)} is already the shortcut for “${other.label}”. Change or remove it there first.`;
			return;
		}
		stop();
		if (!keys.includes(pressed)) {
			const next = [...keys];
			next[slot] = pressed;
			set(c, next);
		}
		void focusSlot(c.command, slot);
	}

	async function resetAll() {
		const choice = await app.ask({
			title: 'Reset all shortcuts?',
			message: 'Every command gets back the shortcuts it came with. The ones you changed are lost.',
			buttons: [
				{ id: 'reset', label: 'Reset shortcuts', primary: true },
				{ id: 'cancel', label: 'Cancel' }
			],
			cancel: 'cancel'
		});
		if (choice !== 'reset') return;
		draft.shortcuts = {};
		commit();
	}
</script>

<div bind:this={root}>
	<div class="flex items-center gap-2">
		<div class="field shortcut-search flex h-[32px] min-w-0 flex-1 items-center gap-2 px-2">
			<Search size={14} class="shrink-0 text-fg-muted" aria-hidden="true" />
			<input
				id="setting-shortcuts"
				type="search"
				class="min-w-0 flex-1 bg-transparent text-[13px] outline-none"
				placeholder="Search shortcuts"
				aria-label="Search shortcuts"
				bind:value={query}
			/>
		</div>
		<button type="button" class="button" disabled={!changed} onclick={() => void resetAll()}>Reset all&hellip;</button>
	</div>
	<p class="setting-note mt-2 mb-4">
		Click a shortcut, then press its new keys; the plus adds another. Backspace removes it, Esc keeps it.
	</p>

	{#each groups as g (g.title)}
		<section class="settings-section" aria-label={g.title}>
			<h3 class="settings-section-title">{g.title}</h3>
			<div class="flex flex-col">
				{#each g.commands as c (c.command)}
					{@const keys = keysOf(c.command, draft.shortcuts)}
					{@const active = recording?.command === c.command ? recording.slot : -1}
					<!-- The keys it has, and one more place while a key is being added. -->
					{@const slots = active === keys.length ? keys.length + 1 : keys.length}
					<div class="shortcut-row" data-command={c.command}>
						<span class="min-w-0 flex-1 text-[13px]">{c.label}</span>
						<div class="flex shrink-0 flex-wrap items-center justify-end gap-1">
							{#each { length: slots }, i (i)}
								{@const k = keys[i]}
								<span class="shortcut-chip" class:shortcut-chip-recording={active === i}>
									<button
										type="button"
										class="shortcut-key"
										data-slot={i}
										aria-label={active === i ? `Press the shortcut for ${c.label}` : `${c.label}: ${displayShortcut(k ?? '')}. Change`}
										data-tip={active === i ? undefined : 'Change'}
										onclick={() => record(c, i)}
										onkeydown={(e) => onKey(e, c, i)}
										onblur={() => {
											if (active === i) stop();
										}}
									>
										{active === i ? 'Press keys…' : displayShortcut(k ?? '')}
									</button>
									<!-- One that was added (not one the command came with) can be taken away. -->
									{#if k !== undefined && active !== i && !defaultKeys(c).includes(k)}
										<button
											type="button"
											class="shortcut-remove"
											aria-label="Remove {displayShortcut(k)} from {c.label}"
											data-tip="Remove"
											onclick={() => {
												stop();
												set(c, keys.filter((_, j) => j !== i));
												void focusSlot(c.command, i);
											}}
										>
											<X size={12} aria-hidden="true" />
										</button>
									{/if}
								</span>
							{/each}
							<button
								type="button"
								class="icon-button size-6"
								class:invisible={keys.length >= MAX_KEYS}
								disabled={keys.length >= MAX_KEYS}
								data-add
								aria-label="Add a shortcut for {c.label}"
								data-tip="Add a shortcut"
								onclick={() => record(c, keys.length)}
							>
								<Plus size={14} aria-hidden="true" />
							</button>
						</div>
						{#if active >= 0 && problem}
							<p class="shortcut-problem" role="alert">{problem}</p>
						{/if}
					</div>
				{/each}
			</div>
		</section>
	{/each}

	{#if fixed.length > 0}
		<section class="settings-section" aria-label="Fixed keys">
			<h3 class="settings-section-title">Fixed keys</h3>
			<p class="setting-note">These work where the focus is and can&rsquo;t be changed.</p>
			<div class="flex flex-col">
				{#each fixed as f (f.keys)}
					<div class="shortcut-row">
						<span class="min-w-0 flex-1 text-[13px]">{f.label}</span>
						<span class="shortcut-chip shortcut-chip-fixed"><span class="shortcut-key">{displayShortcut(f.keys)}</span></span>
					</div>
				{/each}
			</div>
		</section>
	{/if}

	{#if groups.length === 0 && fixed.length === 0}
		<p class="setting-note mt-6" role="status">No shortcut matches &ldquo;{query}&rdquo;.</p>
	{/if}
</div>
