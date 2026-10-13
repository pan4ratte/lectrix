// `npm run test:ci`: the end-to-end tests with the app shown as CI's Windows runners show
// it (LECTRIX_E2E_LIKE_CI in app.mjs: reduced motion, and a window of their size), so that
// what only fails there fails here first. Test files can be named, as paths from
// tests/e2e:
//
//   npm run test:ci --prefix tests/e2e -- specs/motion.test.mjs

import { spawnSync } from 'node:child_process';

import { trimDriverLog } from './driverlog.mjs';

const files = process.argv.slice(2);
// A full run starts the driver's log afresh, as `npm test` does.
if (files.length === 0) trimDriverLog(0);
const run = spawnSync(process.execPath, ['--test', '--test-concurrency=1', ...(files.length > 0 ? files : ['specs/*.test.mjs'])], {
	stdio: 'inherit',
	env: { ...process.env, LECTRIX_E2E_LIKE_CI: '1' }
});
process.exit(run.status ?? 1);
