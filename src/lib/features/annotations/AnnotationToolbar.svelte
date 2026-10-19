<script lang="ts">
	// The annotation toolbar (sections 6.5 and 8): the colour every tool draws with, before the
	// tools, so it can be picked before a tool is; then the tools, and the rest of the style
	// the active tool draws with (remembered per tool: the opacity, in the colour's panel, the
	// stroke width, the font size).
	// Settings dock it, always shown: in the bar at the top of the document, beside the page
	// and zoom controls (ViewBar.svelte places it), or in a bar of its own at the left or the
	// right of the pages (App.svelte); or make it float over the page canvas, at any of its
	// four edges, shown always or only while the pointer is near that edge (or the toolbar has
	// focus, or a tool was just picked, or something it opened is open). At a side, docked or floating, its controls are
	// stacked, and what they open (the colours, a list of sizes) goes beside it, over the pages.
	// Full screen (section 6.1) it always floats, with the page and zoom controls before the
	// tools and a button that leaves full screen after them; Alt held shows it too.
	import {
		Highlighter,
		Minimize,
		PenLine,
		StickyNote,
		Strikethrough,
		TextCursor,
		Type,
		Underline,
		ZodiacAquarius
	} from '@lucide/svelte';
	import { onMount, type Component } from 'svelte';
	import { fade, slide } from 'svelte/transition';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import { motionMs } from '#lib/components/panes.ts';
	import { toggleFullScreen } from '#lib/features/fullscreen/actions.ts';
	import { fullscreen } from '#lib/features/fullscreen/fullscreen.svelte.ts';
	import PageControls from '#lib/features/viewer/PageControls.svelte';
	import type { ToolbarDock, ToolbarPosition } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { refuseIfLocked } from './actions.ts';
	import { isStacked, menuOffset, menuSide, nearEdge } from './bars.ts';
	import ColorPicker from './ColorPicker.svelte';
	import { tools } from './state.svelte.ts';
	import { GLIDE_MS, toolMark } from './toolMark.ts';
	import { FONT_SIZES, PEN_WIDTHS, TOOLS, ptOptions, type Tool } from './tools.ts';

	/** `dock`: where it is docked (in the bar at the top of the document, or a bar at a side
	 * of the pages), rather than floating over the pages. */
	let { tab, dock = null }: { tab: DocTab; dock?: ToolbarDock | null } = $props();

	const ICONS: Record<Tool, Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>> = {
		select: TextCursor,
		highlight: Highlighter,
		underline: Underline,
		strikeOut: Strikethrough,
		squiggly: ZodiacAquarius,
		note: StickyNote,
		ink: PenLine,
		freeText: Type
	};

	const active = $derived(tools.tool);
	const drawTool = $derived(active === 'select' ? null : active);
	const style = $derived(drawTool ? tools.style(drawTool) : null);
	const locked = $derived(!tab.flags.canAnnotate);

	const panel = $derived(dock !== null);
	/** Full screen, this is the only bar: the page and zoom controls are in it. */
	const full = $derived(fullscreen.started && !panel);
	const position = $derived(app.settings?.toolbarPosition ?? 'bottom');
	/** The edge of the pages it is at. */
	const edge: ToolbarPosition = $derived(dock ?? position);
	/** At a side, its controls are stacked, and what they open goes beside it. */
	const stacked = $derived(isStacked(edge));
	const side = $derived(menuSide(edge));

	/** The style controls after the tools slide open and shut along the toolbar as the tool changes, as the
	 * mark glides to it (not at all with reduced motion). */
	const reveal = () => ({ axis: stacked ? ('y' as const) : ('x' as const), duration: motionMs(GLIDE_MS) });

	function pick(tool: Tool) {
		if (tool !== 'select' && refuseIfLocked(tab)) return;
		tools.tool = tool;
		tab.viewer?.focus();
	}

	// ----- showing on demand -----

	/** How long a tool picked from the keyboard keeps the toolbar shown. */
	const FLASH_MS = 1500;
	/** How long the toolbar stays after the pointer leaves, so it doesn't flicker. */
	const LINGER_MS = 300;

	const onHover = $derived(!panel && app.settings?.toolbarVisibility === 'onHover');

	let bar: HTMLDivElement | undefined = $state();
	let near = $state(false);
	let focused = $state(false);
	let flash = $state(false);
	/** A panel or list one of its controls opened is open: it lies outside the toolbar, and
	 * would be left floating by itself if the toolbar went. */
	let opened = $state(false);
	let shown = $state(true);
	const wanted = $derived(!onHover || near || focused || flash || opened || fullscreen.peek || fullscreen.justEntered);

	$effect(() => {
		const el = bar;
		if (!el) return;
		// Every control that opens something says so on itself (Bits UI's triggers).
		const check = () => (opened = el.querySelector('[aria-expanded="true"]') !== null);
		check();
		const watcher = new MutationObserver(check);
		watcher.observe(el, { subtree: true, attributes: true, attributeFilter: ['aria-expanded'] });
		return () => watcher.disconnect();
	});

	let lingerTimer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		if (wanted) {
			clearTimeout(lingerTimer);
			lingerTimer = undefined;
			shown = true;
		} else if (shown && lingerTimer === undefined) {
			lingerTimer = setTimeout(() => {
				lingerTimer = undefined;
				shown = false;
			}, LINGER_MS);
		}
	});

	function onWindowPointerMove(event: PointerEvent) {
		const area = bar?.parentElement?.getBoundingClientRect();
		if (!onHover || !area) return;
		const inside = nearEdge(area, event.clientX, event.clientY, position);
		// A drag (selecting, drawing) reaching the edge doesn't bring the toolbar up.
		if (inside && !near && event.buttons !== 0) return;
		near = inside;
	}

	let flashTimer: ReturnType<typeof setTimeout> | undefined;
	let lastTool = tools.tool;
	$effect(() => {
		const tool = tools.tool;
		if (tool === lastTool) return;
		lastTool = tool;
		flash = true;
		clearTimeout(flashTimer);
		flashTimer = setTimeout(() => (flash = false), FLASH_MS);
	});

	onMount(() => {
		// The pointer leaving the window over the toolbar's edge.
		const leave = () => (near = false);
		document.documentElement.addEventListener('mouseleave', leave);
		return () => {
			document.documentElement.removeEventListener('mouseleave', leave);
			clearTimeout(lingerTimer);
			clearTimeout(flashTimer);
		};
	});
</script>

<svelte:window onpointermove={onWindowPointerMove} />

{#snippet rule()}
	<span class={stacked ? 'my-1 h-px w-6 shrink-0 bg-line' : 'mx-1 h-6 w-px shrink-0 bg-line'} aria-hidden="true"></span>
{/snippet}

<div
	bind:this={bar}
	class={[
		'flex items-center gap-0.5',
		stacked && 'flex-col',
		!panel && [
			'annotation-toolbar z-20 rounded-panel border border-line bg-surface-raised p-1 shadow-[0_4px_12px_var(--color-page-shadow)]',
			`annotation-toolbar-${edge}`
		]
	]}
	class:annotation-toolbar-hidden={!shown}
	role="toolbar"
	aria-orientation={stacked ? 'vertical' : undefined}
	aria-label={full ? 'Page, zoom and annotation tools' : 'Annotation tools'}
	onfocusin={() => (focused = true)}
	onfocusout={(e) => (focused = bar?.contains(e.relatedTarget as Node | null) ?? false)}
	transition:fade={{ duration: motionMs(GLIDE_MS) }}
>
	{#if full}
		<span class={['flex items-center gap-0.5', stacked && 'flex-col']} transition:slide={reveal()}>
			<PageControls {tab} nested {side} />
			{@render rule()}
		</span>
	{/if}

	<ColorPicker
		color={tools.color}
		opacity={style?.opacity ?? null}
		label="Colour"
		{side}
		oncolor={(color) => tools.setColor(color)}
		onopacity={(opacity) => drawTool && tools.setStyle(drawTool, { opacity })}
	/>
	{@render rule()}

	<span class={['tool-group flex items-center gap-0.5', stacked && 'flex-col']} {@attach toolMark}>
		<span class="tool-mark" aria-hidden="true" hidden></span>
		{#each TOOLS as t (t.id)}
			{@const Icon = ICONS[t.id]}
			<button
				type="button"
				class="icon-button tool-button"
				aria-pressed={active === t.id}
				aria-label={t.label}
				data-tip={t.key ? `${t.label} (${t.key})` : t.label}
				aria-disabled={t.id !== 'select' && locked}
				onclick={() => pick(t.id)}
			>
				<Icon size={18} aria-hidden="true" />
			</button>
		{/each}
	</span>

	{#if style && (drawTool === 'ink' || drawTool === 'freeText')}
		<span class={['flex items-center gap-0.5', stacked && 'flex-col']} transition:slide={reveal()}>
			{@render rule()}
			{#if drawTool === 'ink'}
				<span class="flex" transition:slide={reveal()}>
					<Dropdown
						class={stacked ? 'toolbar-dropdown toolbar-dropdown-stacked' : 'toolbar-dropdown'}
						label="Stroke width"
						tip="Stroke width"
						value={style.width}
						options={ptOptions(PEN_WIDTHS)}
						{side}
						sideOffset={menuOffset(side, 4)}
						compact={stacked}
						onchange={(width) => tools.setStyle('ink', { width })}
					/>
				</span>
			{/if}
			{#if drawTool === 'freeText'}
				<span class="flex" transition:slide={reveal()}>
					<Dropdown
						class={stacked ? 'toolbar-dropdown toolbar-dropdown-stacked' : 'toolbar-dropdown'}
						label="Font size"
						tip="Font size"
						value={style.fontSize}
						options={ptOptions(FONT_SIZES)}
						{side}
						sideOffset={menuOffset(side, 4)}
						compact={stacked}
						onchange={(fontSize) => tools.setStyle('freeText', { fontSize })}
					/>
				</span>
			{/if}
		</span>
	{/if}

	{#if full}
		<span class={['flex items-center gap-0.5', stacked && 'flex-col']} transition:slide={reveal()}>
			{@render rule()}
			<button
				type="button"
				class="icon-button"
				aria-label="Leave full screen"
				data-tip={app.hint('Leave full screen', 'fullScreen')}
				onclick={() => void toggleFullScreen()}
			>
				<Minimize size={18} aria-hidden="true" />
			</button>
		</span>
	{/if}
</div>
