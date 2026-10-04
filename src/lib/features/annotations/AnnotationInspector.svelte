<script lang="ts">
	// Inspector for the selected annotation (sections 6.5 and 8): colour, opacity, note text,
	// author and dates. It floats over the page canvas, like the bookmark inspector, and is
	// shown while an annotation is selected.
	import { Trash, TriangleAlert, Wrench, X } from '@lucide/svelte';

	import { stopUnlessShortcut } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { remove, repair, update } from './actions.ts';
	import {
		FONT_SIZES,
		PEN_WIDTHS,
		PRESET_COLORS,
		PROBLEM_TEXT,
		capabilities,
		formatDate,
		typeName
	} from './tools.ts';

	let { tab }: { tab: DocTab } = $props();

	const OPACITIES = [1, 0.8, 0.6, 0.4, 0.2];

	const a = $derived(tab.selectedAnnotationInfo);
	const caps = $derived(a ? capabilities(a, tab.flags.canAnnotate) : null);
	const replies = $derived(a ? tab.repliesTo(a.page, a.id) : []);
	const parent = $derived(a?.replyTo != null ? tab.annotation(a.page, a.replyTo) : null);
	const label = $derived(a ? (tab.displayLabels?.[a.page] ?? String(a.page + 1)) : '');

	let noteField: HTMLTextAreaElement | undefined = $state();

	// A note just placed with the Note tool: type its text right away.
	$effect(() => {
		if (app.focusNoteText && noteField) {
			app.focusNoteText = false;
			noteField.focus();
		}
	});

	function commitText(event: Event) {
		if (!a) return;
		const value = (event.currentTarget as HTMLTextAreaElement).value;
		if (value !== a.contents) void update(tab, a.page, a.id, { contents: value });
	}

	function commitAuthor(event: Event) {
		if (!a) return;
		const value = (event.currentTarget as HTMLInputElement).value.trim();
		if (value && value !== a.author) void update(tab, a.page, a.id, { author: value });
		else (event.currentTarget as HTMLInputElement).value = a.author;
	}

	function textKey(event: KeyboardEvent) {
		stopUnlessShortcut(event);
		if (event.key === 'Enter' && event.ctrlKey) {
			event.preventDefault();
			(event.currentTarget as HTMLElement).blur();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			(event.currentTarget as HTMLTextAreaElement).value = a?.contents ?? '';
			(event.currentTarget as HTMLElement).blur();
		}
	}

	function authorKey(event: KeyboardEvent) {
		stopUnlessShortcut(event);
		if (event.key === 'Enter') {
			event.preventDefault();
			(event.currentTarget as HTMLElement).blur();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			(event.currentTarget as HTMLInputElement).value = a?.author ?? '';
			(event.currentTarget as HTMLElement).blur();
		}
	}

	function close() {
		tab.selectedAnnotation = null;
		tab.viewer?.focus();
	}
</script>

{#if a && caps}
	<aside
		class="absolute right-6 z-10 flex max-h-[calc(100%-24px)] w-72 flex-col gap-3 overflow-y-auto rounded-panel border border-line bg-surface-raised p-3 shadow-[0_4px_12px_var(--color-page-shadow)]"
		style:top={tab.search.open ? '64px' : '12px'}
		aria-label="Annotation properties"
	>
		<div class="flex items-center gap-2">
			{#if a.color}
				<span class="annotation-dot" style:--swatch={a.color} aria-hidden="true"></span>
			{/if}
			<h2 class="flex-1 text-sm font-semibold">
				{typeName(a.subtype)}<span class="font-normal text-fg-muted"> · page {label}</span>
			</h2>
			<button type="button" class="icon-button" aria-label="Close properties" onclick={close}>
				<X size={16} aria-hidden="true" />
			</button>
		</div>

		{#if !tab.flags.canAnnotate}
			<p class="text-xs text-fg-muted">This document’s security settings don’t allow changing annotations.</p>
		{:else if a.id === 0}
			<p class="text-xs text-fg-muted">This annotation is stored in a way Lectrix can show but not change.</p>
		{/if}

		{#if parent}
			<p class="text-xs text-fg-muted">
				Reply to {typeName(parent.subtype).toLowerCase()} by {parent.author || 'unknown'}. Replies are read-only.
			</p>
		{/if}

		{#if caps.restyle}
			<div class="flex flex-col gap-1">
				<span class="text-xs text-fg-muted">{a.kind === 'freeText' ? 'Text colour' : 'Colour'}</span>
				<div class="flex items-center gap-1" role="radiogroup" aria-label="Colour">
					{#each PRESET_COLORS as c (c.value)}
						<button
							type="button"
							class="swatch"
							role="radio"
							aria-checked={a.color === c.value}
							aria-label={c.name}
							title={c.name}
							style:--swatch={c.value}
							onclick={() => void update(tab, a.page, a.id, { color: c.value })}
						></button>
					{/each}
					<label
						class="swatch swatch-custom"
						class:swatch-custom-on={a.color !== null && !PRESET_COLORS.some((c) => c.value === a.color)}
						title="Custom colour"
					>
						<span class="sr-only">Custom colour</span>
						<input
							type="color"
							class="sr-only"
							value={a.color ?? '#000000'}
							onchange={(e) => void update(tab, a.page, a.id, { color: e.currentTarget.value })}
						/>
					</label>
				</div>
			</div>
			<div class="flex gap-3">
				<label class="flex flex-1 flex-col gap-1">
					<span class="text-xs text-fg-muted">Opacity</span>
					<select
						class="field"
						value={OPACITIES.reduce((best, o) => (Math.abs(o - a.opacity) < Math.abs(best - a.opacity) ? o : best))}
						onchange={(e) => void update(tab, a.page, a.id, { opacity: Number(e.currentTarget.value) })}
					>
						{#each OPACITIES as o (o)}
							<option value={o}>{Math.round(o * 100)}%</option>
						{/each}
					</select>
				</label>
				{#if a.kind === 'ink'}
					<label class="flex flex-1 flex-col gap-1">
						<span class="text-xs text-fg-muted">Stroke width</span>
						<select
							class="field"
							value={a.width ?? 1}
							onchange={(e) => void update(tab, a.page, a.id, { width: Number(e.currentTarget.value) })}
						>
							{#each [...new Set([...PEN_WIDTHS, a.width ?? 1])].sort((x, y) => x - y) as w (w)}
								<option value={w}>{w} pt</option>
							{/each}
						</select>
					</label>
				{/if}
				{#if a.kind === 'freeText'}
					<label class="flex flex-1 flex-col gap-1">
						<span class="text-xs text-fg-muted">Font size</span>
						<select
							class="field"
							value={a.fontSize ?? 12}
							onchange={(e) => void update(tab, a.page, a.id, { fontSize: Number(e.currentTarget.value) })}
						>
							{#each [...new Set([...FONT_SIZES, a.fontSize ?? 12])].sort((x, y) => x - y) as s (s)}
								<option value={s}>{s} pt</option>
							{/each}
						</select>
					</label>
				{/if}
			</div>
		{/if}

		<label class="flex flex-col gap-1">
			<span class="text-xs text-fg-muted">{a.kind === 'freeText' ? 'Text' : 'Note'}</span>
			{#key `${a.page}:${a.id}`}
				<textarea
					bind:this={noteField}
					class="field min-h-20 resize-y py-1"
					value={a.contents}
					readonly={!caps.text}
					placeholder={caps.text ? 'Add a note' : ''}
					onkeydown={textKey}
					onblur={commitText}
				></textarea>
			{/key}
		</label>

		<label class="flex flex-col gap-1">
			<span class="text-xs text-fg-muted">Author</span>
			{#key `${a.page}:${a.id}`}
				<input class="field h-8" value={a.author} readonly={!caps.text} onkeydown={authorKey} onblur={commitAuthor} />
			{/key}
		</label>

		<dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
			<dt class="text-fg-muted">Created</dt>
			<dd>{formatDate(a.created)}</dd>
			<dt class="text-fg-muted">Modified</dt>
			<dd>{formatDate(a.modified)}</dd>
		</dl>

		{#if replies.length}
			<div class="flex flex-col gap-1">
				<span class="text-xs text-fg-muted">Replies</span>
				{#each replies as r (r.id)}
					<div class="rounded-control border border-line p-2 text-xs">
						<p class="font-semibold">{r.author || 'Unknown'}</p>
						<p class="break-words whitespace-pre-wrap select-text">{r.contents}</p>
					</div>
				{/each}
			</div>
		{/if}

		{#if a.problems.length}
			<div class="flex flex-col gap-2 rounded-control bg-info-bg p-2 text-xs">
				<p class="flex items-center gap-1 font-semibold">
					<TriangleAlert size={14} aria-hidden="true" />Needs repair
				</p>
				<ul class="list-disc pl-4">
					{#each a.problems as p (p)}
						<li>It {PROBLEM_TEXT[p]}.</li>
					{/each}
				</ul>
				{#if tab.flags.canAnnotate}
					<button type="button" class="button gap-1 self-start" onclick={() => void repair(tab)}>
						<Wrench size={14} aria-hidden="true" />Repair annotations…
					</button>
				{/if}
			</div>
		{/if}

		{#if caps.delete}
			<button type="button" class="button gap-1 self-start" onclick={() => void remove(tab, a.page, a.id)}>
				<Trash size={14} aria-hidden="true" />Delete
			</button>
		{/if}
	</aside>
{/if}
