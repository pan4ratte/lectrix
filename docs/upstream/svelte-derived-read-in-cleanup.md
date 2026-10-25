# Svelte: a $derived read in an effect's cleanup stops updating

To file at https://github.com/sveltejs/svelte/issues (found with svelte 5.57.1; 5.57.2 has
it too).

---

**Title:** `$derived` stays stale for good after an effect's cleanup reads it while its dependencies change

**Describe the bug.** When an effect is torn down in the same flush that changes which
signals a `$derived` depends on, and the effect's cleanup reads the derived, the derived
never updates again. Its value is right at that moment, but later changes to its new
dependencies reach neither the derived nor the effects that read it.

In an app this happens when a component that receives a derived value as a prop goes away
as that value changes: its cleanup reads the prop. In Lectrix (a PDF viewer), the
document shown is `$derived` from the tab shown; switching tabs destroys the old tab's
viewer, whose cleanup reads its `tab` prop, and from then on the derived ignores which
pane of the new tab has the focus.

**Reproduction** (a `.svelte.js` module):

```js
import { flushSync } from 'svelte';

let which = $state(0);
let a = $state(0);
let b = $state(7);
let shown = $state(true);
const active = $derived(which === 0 ? a : b);

const seen = [];
const stop = $effect.root(() => {
	$effect(() => {
		seen.push(active);
	});
	$effect(() => {
		if (!shown) return;
		// A cleanup that reads the derived, as a component going away reads its props.
		return () => active;
	});
});
flushSync();

// One change makes `active` depend on `b` instead of `a`, and tears down the effect that
// reads it on its way out.
which = 1;
shown = false;
flushSync();
console.log(active, seen); // 7 [0, 7]

b = 5;
flushSync();
console.log(active, seen); // expected 5 [0, 7, 5]; prints 7 [0, 7]
stop();
```

**Cause, as far as we can tell.** In `get` (`runtime.js`), a derived read while an
effect is being destroyed is run through `execute_derived` when it is dirty and has
reactions. That replaces its `deps` with the new ones (`b`), and `update_dependencies`
removes it from the old dependencies' reactions, but adds it to the new ones' only when
`effect_tracking()`, which is false in a cleanup. The derived stays `CONNECTED`, so it is
never checked again by version; and when it next runs inside an effect, its dependencies
are the same as the list it already has, so `skipped_deps` covers them all and it is
still not added to `b.reactions`.

**Logs.** None; no warning in development mode (`derived_inert` does not apply: the
derived belongs to no effect).

**System info.** svelte 5.57.1 and 5.57.2, client runtime; Windows 11, WebView2 and Node 24.

**Severity.** Blocking an annoyance: the workaround is not to keep such values in
`$derived` (getters instead), which a cache was meant to spare.
