// A reporter for `node --test` that writes one line of JSON per test: its name, its file,
// whether it passed and, if not, the first line of the error. The soak run (run.mjs) reads
// these to count, over many passes, the tests that fail only some of the time.

/** The first line of what a failed test threw. */
function reason(details) {
	const error = details?.error;
	const message = error?.cause?.message ?? error?.message ?? String(error ?? 'failed');
	return message.split('\n').find((line) => line.trim() !== '') ?? 'failed';
}

export default async function* soakReporter(source) {
	for await (const event of source) {
		if (event.type !== 'test:pass' && event.type !== 'test:fail') continue;
		const { name, file, details } = event.data;
		// A describe() block is not a test; its tests report themselves.
		if (details?.type === 'suite') continue;
		const ok = event.type === 'test:pass';
		yield `${JSON.stringify({ name, file: file ?? null, ok, ...(ok ? {} : { error: reason(details) }) })}\n`;
	}
}
