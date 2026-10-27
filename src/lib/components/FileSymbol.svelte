<script lang="ts">
	// The recent files list's file icon: the brand's letter, 18 px, in a 24 px rounded square,
	// its tails running off the square's right edge as they run off the app icon; one colour
	// (currentColor) and no fill. Copied from the brand kit's
	// design/file-icon/lectrix-file-symbol.svg, with an id of its own for each copy on the page.
	const uid = $props.id();
</script>

<svg viewBox="0 0 24 24" width="24" height="24" fill="none" aria-hidden="true">
	<defs>
		<clipPath id="{uid}-in"><rect x="0.5" y="0.5" width="23" height="23" rx="5.2" /></clipPath>
	</defs>
	<rect x="0.5" y="0.5" width="23" height="23" rx="5.2" stroke="currentColor" />
	<g clip-path="url(#{uid}-in)">
		<g transform="translate(3 3) scale(0.04839) translate(-112 -52)">
			<path
				transform="translate(14 -14)"
				stroke="currentColor"
				stroke-width="33"
				stroke-linecap="round"
				stroke-linejoin="round"
				d="M540,60 C400,30 300,80 270,200 C258,250 246,300 222,330 C206,362 160,384 148,356 C136,328 186,316 222,332 C300,370 400,414 540,424"
			/>
		</g>
	</g>
</svg>
