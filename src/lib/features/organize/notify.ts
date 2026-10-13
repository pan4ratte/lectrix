// What is said after pages are deleted (section 6.7), for the grid and the Debug menu.

import type { DeleteReport } from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';

import { deletedMessage } from './pages.ts';

/** Says how many pages were deleted and what went with them, and offers the save that
 * takes them out of the file (`saveOptimized`). */
export function notifyDeleted(report: DeleteReport, saveOptimized: () => void) {
	app.notify(
		{ kind: 'info', ...deletedMessage(report), action: { label: 'Save As (optimized)…', run: saveOptimized } },
		30000
	);
}
