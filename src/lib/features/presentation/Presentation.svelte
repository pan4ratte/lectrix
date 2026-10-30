<script lang="ts">
	// Presentation (section 6.1): one page at a time, as large as fits the screen, on black.
	// On one screen, the pages show here: a small bar of controls shows in the middle of the
	// bottom while the pointer moves, and the pointer hides once it rests. With the pages on
	// another screen (audience.rs), this window shows the presenter view, and keeps that
	// screen told what to show. Over the whole window, under dialogs and notifications.
	import { ChevronLeft, ChevronRight, X } from '@lucide/svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { onMount, tick } from 'svelte';
	import { fade } from 'svelte/transition';

	import { motionMs } from '#lib/components/panes.ts';
	import { follow } from '#lib/features/viewer/links.ts';
	import { pagePosition } from '#lib/features/viewer/pagebox.ts';
	import { imageFormatInUse, onAudienceClosed, onWindowMessage, sendToWindow } from '#lib/ipc/index.ts';
	import { commandFor } from '#lib/shortcuts.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { stageLook, stagePage } from './build.ts';
	import { StageGestures } from './gestures.ts';
	import { actionFor } from './input.ts';
	import PageStage from './PageStage.svelte';
	import PresenterView from './PresenterView.svelte';
	import { FADE_MS, presentation } from './presentation.svelte.ts';
	import { INPUT_EVENT, STAGE_EVENT, audienceLabel, type AudienceInput } from './relay.ts';
	import { keptPages, linkIndexAt, type StageState } from './stage.ts';

	/** How long the pointer rests before it hides, with the controls. */
	const IDLE_MS = 2500;

	let { tab }: { tab: DocTab } = $props();

	let overlay: HTMLDivElement | undefined = $state();
	let controls: HTMLDivElement | undefined = $state();
	let measuredW = $state(0);
	let measuredH = $state(0);

	/** The screen the pages are laid out for: the window once it fills the screen; before,
	 * the screen it will fill, so the pages render at their final size from the start. */
	const screenW = $derived(presentation.shown && measuredW > 0 ? measuredW : window.screen.width);
	const screenH = $derived(presentation.shown && measuredH > 0 ? measuredH : window.screen.height);

	const current = $derived(Math.min(presentation.page, tab.pageCount - 1));

	/** What the pages' screen shows: this one, or the audience screen. */
	const stage: StageState = $derived({
		docId: tab.id,
		revision: tab.state.revision,
		imageFormat: imageFormatInUse(),
		current,
		pages: keptPages(current, tab.pageCount, presentation.change.from).map((index) => stagePage(tab, index)),
		blank: presentation.blank,
		look: stageLook(tab),
		change: presentation.change,
		shown: presentation.shown
	});

	const position = $derived(pagePosition(current, tab.pageCount, tab.displayLabels));
	const pageLabel = (index: number) => `Page ${pagePosition(index, tab.pageCount, tab.displayLabels)}`;

	// The pages' links: the page shown, and those around it.
	$effect(() => {
		for (const page of keptPages(current, tab.pageCount, null)) void tab.loadText(page).catch(() => {});
	});

	// ----- the audience screen -----

	const audience = audienceLabel(getCurrentWindow().label);

	// It is told what to show whenever that changes.
	$effect(() => {
		if (presentation.audience) void sendToWindow(audience, STAGE_EVENT, $state.snapshot(stage)).catch(() => {});
	});

	function fromAudience(input: AudienceInput) {
		if (!presentation.audience) return;
		switch (input.kind) {
			case 'ready':
				void sendToWindow(audience, STAGE_EVENT, $state.snapshot(stage)).catch(() => {});
				return;
			case 'key':
				onKey(input, null);
				return;
			case 'step':
				presentation.step(input.direction);
				return;
			case 'link':
				if (input.page === current) followLink(input.link);
				return;
		}
	}

	// ----- the pointer, and the controls -----

	/** The pointer moved a moment ago: it shows, and so do the controls. */
	let pointerActive = $state(false);
	let overControls = $state(false);
	let controlsFocused = $state(false);
	let idle: ReturnType<typeof setTimeout> | undefined;
	let lastX = -1;
	let lastY = -1;
	/** The link under the pointer on the page shown. */
	let overLink = $state(false);
	const controlsShown = $derived(presentation.shown && (pointerActive || overControls || controlsFocused || presentation.typed !== ''));

	function wake() {
		pointerActive = true;
		clearTimeout(idle);
		idle = setTimeout(() => (pointerActive = false), IDLE_MS);
	}

	function linkAt(x: number, y: number): number | null {
		const page = stage.pages.find((p) => p.index === current);
		return page && !presentation.blank ? linkIndexAt(page, screenW, screenH, x, y) : null;
	}

	function followLink(index: number) {
		const link = tab.text(current)?.links[index];
		if (!link) return;
		if (link.target.kind === 'page') presentation.goTo(link.target.page);
		else follow(tab, link.target, { kind: 'page', page: current });
	}

	function onPointerMove(event: PointerEvent) {
		// Pages turning under a pointer at rest send moves without movement.
		if (event.clientX === lastX && event.clientY === lastY) return;
		lastX = event.clientX;
		lastY = event.clientY;
		if (event.pointerType === 'touch' || presentation.audience) return;
		wake();
		overLink = linkAt(event.clientX, event.clientY) !== null;
	}

	// A page turned under the pointer: the link it was on is gone.
	$effect(() => {
		void current;
		overLink = false;
	});

	const onControls = (target: EventTarget | null) => target instanceof Node && (controls?.contains(target) ?? false);

	const gestures = new StageGestures({
		active: () => presentation.shown && !presentation.audience,
		linkAt,
		step: (direction) => presentation.step(direction),
		follow: followLink,
		onControl: onControls
	});

	// ----- keys -----

	const buttons = () => [...(overlay?.querySelectorAll<HTMLButtonElement>('[data-presentation-controls] button:not(:disabled)') ?? [])];

	/** Tab moves among the controls, which show for it, and stays among them. */
	async function moveFocus(back: boolean) {
		const list = buttons();
		if (list.length === 0) return;
		const at = list.indexOf(document.activeElement as HTMLButtonElement);
		const next = at < 0 ? (back ? list.length - 1 : 0) : (at + (back ? -1 : 1) + list.length) % list.length;
		controlsFocused = true;
		await tick();
		list[next]!.focus();
	}

	type KeyLike = Pick<KeyboardEvent, 'key' | 'code' | 'shiftKey' | 'ctrlKey' | 'altKey' | 'metaKey'>;

	/** A key, here (`event`) or on the audience screen: the presentation's. */
	function onKey(key: KeyLike, event: KeyboardEvent | null) {
		if (commandFor(key, false) === 'presentation') {
			void presentation.stop();
			return;
		}
		if (!presentation.shown) {
			if (key.key === 'Escape') void presentation.stop();
			return;
		}
		const target = event?.target;
		if (event && key.key === 'Tab') {
			void moveFocus(key.shiftKey);
			return;
		}
		// A control with the focus answers Enter and Space itself.
		if ((key.key === 'Enter' || key.key === ' ') && target instanceof HTMLButtonElement && overlay?.contains(target)) {
			target.click();
			return;
		}
		const action = actionFor(key);
		if (action) presentation.act(action);
	}

	/** Every key is the presentation's, before the app's shortcuts see it; but a dialog or a
	 * menu that opened over it has its own. */
	function onWindowKey(event: KeyboardEvent) {
		const target = event.target;
		if (target instanceof Element && !overlay?.contains(target) && target.closest('[role=dialog], [role=alertdialog], [role=menu]')) return;
		event.stopPropagation();
		// Alt+F4 still closes the window.
		if (!(event.altKey && event.key === 'F4')) event.preventDefault();
		onKey(event, event);
	}

	onMount(() => {
		window.addEventListener('keydown', onWindowKey, { capture: true });
		overlay?.focus();
		const unlisten = [
			onWindowMessage<AudienceInput>(INPUT_EVENT, fromAudience),
			// Closed on its screen (Alt+F4 there): the presentation ends with it.
			onAudienceClosed(() => {
				if (presentation.audience) void presentation.stop();
			})
		];
		return () => {
			window.removeEventListener('keydown', onWindowKey, { capture: true });
			clearTimeout(idle);
			for (const u of unlisten) void u.then((f) => f());
		};
	});
</script>

<!-- Keys are taken for the whole window, while it shows (onWindowKey). -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
	bind:this={overlay}
	bind:clientWidth={measuredW}
	bind:clientHeight={measuredH}
	class={['presentation fixed inset-0 z-30 overflow-hidden outline-none', presentation.audience ? 'bg-canvas' : 'bg-presentation']}
	style:cursor={presentation.audience || overControls ? 'default' : !pointerActive ? 'none' : overLink ? 'pointer' : 'default'}
	role="dialog"
	aria-modal="true"
	aria-label="Presentation"
	tabindex="-1"
	data-presentation={presentation.stage}
	data-presenter-view={presentation.audience ? '' : undefined}
	onpointermove={onPointerMove}
	onpointerdown={gestures.pointerDown}
	onpointerup={gestures.pointerUp}
	onpointercancel={gestures.pointerCancel}
	onclick={gestures.click}
	oncontextmenu={gestures.contextMenu}
	onwheel={gestures.wheel}
	transition:fade={{ duration: motionMs(FADE_MS) }}
>
	{#if presentation.audience}
		<PresenterView {tab} {stage} onFollow={followLink} />
	{:else}
		<PageStage {stage} width={screenW} height={screenH} label={pageLabel} />

		<!-- A control clicked leaves the focus where it was: Enter goes on turning pages, and
		     the bar hides once the pointer rests. -->
		<div
			bind:this={controls}
			class="presentation-controls absolute bottom-4 left-1/2 z-20 flex -translate-x-1/2 items-center gap-0.5 rounded-panel border border-line bg-surface-raised p-1 text-fg shadow-[0_4px_12px_var(--color-page-shadow)]"
			class:presentation-controls-hidden={!controlsShown}
			role="toolbar"
			aria-label="Presentation"
			tabindex="-1"
			data-presentation-controls
			onpointerenter={() => (overControls = true)}
			onpointerleave={() => (overControls = false)}
			onmousedown={(e) => e.preventDefault()}
			onfocusin={() => (controlsFocused = true)}
			onfocusout={(e) => (controlsFocused = controls?.contains(e.relatedTarget as Node | null) ?? false)}
		>
			<button
				type="button"
				class="icon-button"
				aria-label="Previous page"
				data-tip="Previous page"
				disabled={current <= 0}
				onclick={() => presentation.step(-1)}
			>
				<ChevronLeft size={16} aria-hidden="true" />
			</button>
			<span class="min-w-16 px-1 text-center text-sm tabular-nums" aria-live="polite" data-presentation-position>
				{presentation.typed ? `Page ${presentation.typed}` : position}
			</span>
			<button
				type="button"
				class="icon-button"
				aria-label="Next page"
				data-tip="Next page"
				disabled={current >= tab.pageCount - 1}
				onclick={() => presentation.step(1)}
			>
				<ChevronRight size={16} aria-hidden="true" />
			</button>
			<span class="mx-1 h-6 w-px shrink-0 bg-line" aria-hidden="true"></span>
			<button
				type="button"
				class="icon-button"
				aria-label="End presentation"
				data-tip={app.hint('End presentation', 'presentation')}
				onclick={() => void presentation.stop()}
			>
				<X size={16} aria-hidden="true" />
			</button>
		</div>
	{/if}
</div>
