// The visual adjustments Settings give every document (sections 6.1 and 6.6): the pages as
// they are until changed there. Set as settings are applied; a document without
// adjustments of its own is shown with these, and follows them as they change.

import type { PageAdjustments } from '#lib/ipc/index.ts';

import { NO_ADJUSTMENTS } from './adjustments.ts';

export const pageDefaults = $state<{ value: PageAdjustments }>({ value: { ...NO_ADJUSTMENTS } });
