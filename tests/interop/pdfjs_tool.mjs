// pdf.js leg of the interop harness.
//
//   node pdfjs_tool.mjs info   <file.pdf>                       -> JSON on stdout
//   node pdfjs_tool.mjs render <file.pdf> <page> <scale> <on|off> <out.png>
//
// `page` is 1-based. `on|off` controls whether annotation appearances are drawn.

import { readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { AnnotationMode, getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';

const require = createRequire(import.meta.url);
const pdfjsRoot = path.dirname(require.resolve('pdfjs-dist/package.json'));
const asDirUrl = (dir) => pathToFileURL(path.join(pdfjsRoot, dir) + path.sep).href;

async function open(file) {
	const data = new Uint8Array(await readFile(file));
	const task = getDocument({
		data,
		standardFontDataUrl: asDirUrl('standard_fonts'),
		cMapUrl: asDirUrl('cmaps'),
		cMapPacked: true,
		iccUrl: asDirUrl('iccs'),
		verbosity: 0
	});
	const doc = await task.promise;
	// pdf.js 6 tears documents down through the loading task.
	doc.close = () => task.destroy();
	return doc;
}

async function resolveOutline(doc, items) {
	const out = [];
	for (const item of items ?? []) {
		let page = null;
		let dest = item.dest;
		if (typeof dest === 'string') dest = await doc.getDestination(dest);
		if (Array.isArray(dest) && dest[0] != null) {
			page = typeof dest[0] === 'object' ? await doc.getPageIndex(dest[0]) : dest[0];
		}
		out.push({
			title: item.title,
			page,
			// Expanded state: the sign of /Count (absent for an item without children).
			open: item.count ? item.count > 0 : null,
			// View coordinates of the destination ([left, top, zoom] for /XYZ).
			view: Array.isArray(dest) ? dest.slice(2) : null,
			children: await resolveOutline(doc, item.items)
		});
	}
	return out;
}

/** The page (0-based) a destination leads to, or null. */
async function destPage(doc, dest) {
	if (typeof dest === 'string') dest = await doc.getDestination(dest);
	if (!Array.isArray(dest) || dest[0] == null) return null;
	return typeof dest[0] === 'object' ? doc.getPageIndex(dest[0]) : dest[0];
}

async function info(file) {
	const doc = await open(file);
	const annotations = [];
	const links = [];
	for (let i = 1; i <= doc.numPages; i++) {
		const page = await doc.getPage(i);
		for (const a of await page.getAnnotations({ intent: 'display' })) {
			annotations.push({
				page: i,
				subtype: a.subtype,
				rect: a.rect,
				hasAppearance: a.hasAppearance ?? null,
				id: a.id
			});
			if (a.subtype === 'Link') {
				links.push({ page: i, target: a.dest ? await destPage(doc, a.dest) : null, uri: a.url ?? null });
			}
		}
	}
	const result = {
		pages: doc.numPages,
		labels: await doc.getPageLabels(),
		outline: await resolveOutline(doc, await doc.getOutline()),
		annotations,
		links
	};
	await doc.close();
	return result;
}

async function render(file, pageNumber, scale, annotations, out) {
	const doc = await open(file);
	const page = await doc.getPage(pageNumber);
	const viewport = page.getViewport({ scale });
	const { canvas, context } = doc.canvasFactory.create(
		Math.ceil(viewport.width),
		Math.ceil(viewport.height)
	);
	context.fillStyle = '#ffffff';
	context.fillRect(0, 0, canvas.width, canvas.height);
	await page.render({
		canvas,
		canvasContext: context,
		viewport,
		annotationMode: annotations ? AnnotationMode.ENABLE : AnnotationMode.DISABLE,
		background: '#ffffff'
	}).promise;
	await writeFile(out, canvas.toBuffer('image/png'));
	await doc.close();
}

const [cmd, file, ...rest] = process.argv.slice(2);
try {
	if (cmd === 'info') {
		process.stdout.write(JSON.stringify(await info(file)));
	} else if (cmd === 'render') {
		const [page, scale, mode, out] = rest;
		await render(file, Number(page), Number(scale), mode === 'on', out);
	} else {
		throw new Error(`unknown command ${cmd}`);
	}
} catch (e) {
	process.stderr.write(`pdfjs_tool: ${e?.stack ?? e}\n`);
	process.exit(1);
}
