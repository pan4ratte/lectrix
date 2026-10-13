// The verbose log msedgedriver writes of every session (target/test-output/e2e/
// driver-session.log, uploaded by CI when the tests fail). The driver appends to it, a
// full run adds some 200 MB, and nothing else ever shortened it: it had grown to 13.9 GB.
// So `npm test` starts it afresh (this file, run as its `pretest`), which leaves the log
// of the last run, and a launch outside that (one spec file, tests/perf/organize.mjs)
// drops a log that has grown past LIMIT.

import { rmSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
export const DRIVER_LOG = join(ROOT, 'target', 'test-output', 'e2e', 'driver-session.log');

/** A few full runs' worth. */
const LIMIT = 1024 ** 3;

/** Removes the log if it is larger than `limit` bytes (0: whatever its size). */
export function trimDriverLog(limit = LIMIT) {
	try {
		if (statSync(DRIVER_LOG).size > limit) rmSync(DRIVER_LOG);
	} catch {
		// No log yet, or a driver still holds it: nothing to do, and no reason to fail.
	}
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) trimDriverLog(0);
