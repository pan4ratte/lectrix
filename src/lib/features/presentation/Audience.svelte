<script lang="ts">
	// An audience screen (section 6.1, audience.rs): a window of its own on a second screen
	// that shows a presentation's pages, as large as fits, on black. What to show comes from
	// the window presenting; the keys, clicks and swipes it gets go back there. The pointer
	// shows only while it moves.
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { onMount } from 'svelte';

	import { onWindowMessage, sendToWindow } from '#lib/ipc/index.ts';

	import { StageGestures } from './gestures.ts';
	import PageStage from './PageStage.svelte';
	import { INPUT_EVENT, STAGE_EVENT, ownerOf, type AudienceInput } from './relay.ts';
	import { linkIndexAt, type StageState } from './stage.ts';

	const IDLE_MS = 2500;

	const owner = ownerOf(getCurrentWindow().label) ?? 'main';

	let stage = $state.raw<StageState | null>(null);
	let width = $state(0);
	let height = $state(0);
	let pointerActive = $state(false);
	let overLink = $state(false);
	let idle: ReturnType<typeof setTimeout> | undefined;

	const send = (input: AudienceInput) => void sendToWindow(owner, INPUT_EVENT, input).catch(() => {});

	function linkAt(x: number, y: number): number | null {
		const s = stage;
		const page = s?.pages.find((p) => p.index === s.current);
		return s && page && !s.blank ? linkIndexAt(page, width, height, x, y) : null;
	}

	const gestures = new StageGestures({
		active: () => stage?.shown === true,
		linkAt,
		step: (direction) => send({ kind: 'step', direction }),
		follow: (link) => {
			if (stage) send({ kind: 'link', page: stage.current, link });
		},
		onControl: () => false
	});

	function onPointerMove(event: PointerEvent) {
		if (event.pointerType === 'touch') return;
		pointerActive = true;
		clearTimeout(idle);
		idle = setTimeout(() => (pointerActive = false), IDLE_MS);
		overLink = linkAt(event.clientX, event.clientY) !== null;
	}

	function onKey(event: KeyboardEvent) {
		// Alt+F4 closes this screen, which ends the presentation.
		if (event.altKey && event.key === 'F4') return;
		event.preventDefault();
		const { key, code, shiftKey, ctrlKey, altKey, metaKey } = event;
		send({ kind: 'key', key, code, shiftKey, ctrlKey, altKey, metaKey });
	}

	onMount(() => {
		const unlisten = onWindowMessage<StageState>(STAGE_EVENT, (s) => (stage = s));
		// Listening first, so what is sent in answer arrives.
		void unlisten.then(() => send({ kind: 'ready' }));
		return () => {
			clearTimeout(idle);
			void unlisten.then((f) => f());
		};
	});
</script>

<svelte:window onkeydown={onKey} />

<!-- Keys are taken for the whole window (onKey). -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
	bind:clientWidth={width}
	bind:clientHeight={height}
	class="fixed inset-0 overflow-hidden bg-presentation"
	style:cursor={!pointerActive ? 'none' : overLink ? 'pointer' : 'default'}
	role="img"
	aria-label="Presentation"
	data-audience
	onpointermove={onPointerMove}
	onpointerdown={gestures.pointerDown}
	onpointerup={gestures.pointerUp}
	onpointercancel={gestures.pointerCancel}
	onclick={gestures.click}
	oncontextmenu={gestures.contextMenu}
	onwheel={gestures.wheel}
>
	{#if stage && width > 0}
		<PageStage {stage} {width} {height} />
	{/if}
</div>
