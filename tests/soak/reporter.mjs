// A reporter for `node --test` that writes one line of JSON per test: its name, its file,
// whether it passed and, if not, the first line of the error. The soak run (run.mjs) reads
// these to count, over many passes, the tests that fail only some of the time.

/** The first line of what a failed test threw, and the next one when the first ends in a
 * colon, as assert's "…did not match the regular expression /x/. Input:" does: what was
 * read is the useful part. Cut to 300 characters, for one cell of the summary's table. */
function reason(details) {
	const error = details?.error;
	const message = error?.cause?.message ?? error?.message ?? String(error ?? 'failed');
	const lines = message.split('\n').map((line) => line.trim()).filter((line) => line !== '');
	if (lines.length === 0) return 'failed';
	const text = lines[0].endsWith(':') && lines.length > 1 ? `${lines[0]} ${lines[1]}` : lines[0];
	return text.length > 300 ? `${text.slice(0, 299)}…` : text;
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
