import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import sharp from 'sharp';
import { published } from '../src/lib/releases.js';
import { metadata, slugs } from '../src/lib/docs/catalog.js';

const at = (path) => fileURLToPath(new URL(path, import.meta.url));
const svg = (body, width = 1200, height = 630) => Buffer.from(
	`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">${body}</svg>`
);
// Inter's variable TTF, pinned to a google/fonts commit. Pango wants the font
// on disk, so the download is cached rather than committed to the repository.
const FONT = {
	url: 'https://cdn.jsdelivr.net/gh/google/fonts@e1d6480102fed30739fead0faee463101f892c8f/ofl/inter/Inter%5Bopsz,wght%5D.ttf',
	sha256: '29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031'
};
const digest = (buffer) => createHash('sha256').update(buffer).digest('hex');

/** The cached Inter, downloaded and checksummed on a miss. */
async function inter() {
	const path = at('../.cache/Inter.ttf');
	const cached = await readFile(path).catch(() => null);
	if (cached && digest(cached) === FONT.sha256) return path;
	const response = await fetch(FONT.url, { signal: AbortSignal.timeout(30_000) });
	if (!response.ok) throw new Error(`OG image: ${response.status} fetching Inter`);
	const font = Buffer.from(await response.arrayBuffer());
	if (digest(font) !== FONT.sha256) throw new Error('OG image: Inter failed its checksum');
	await mkdir(at('../.cache'), { recursive: true });
	await writeFile(path, font);
	return path;
}

const escape = (text) => text.replace(/[&<>"']/g, (char) => ({
	'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;'
})[char]);

const mark = '<path d="M79 98 404 0 316 185Z"/><path d="M162 166 365 261 0 393Z"/>';

/** Render the same brand family without depending on release screenshots. */
export async function renderCard({ title, label, subtitle }, fontfile = undefined) {
	fontfile ??= await inter();
	const text = async (value, size, color = '#eaeaea', width = undefined) => sharp({
		text: {
			text: `<span foreground="${color}">${escape(value)}</span>`,
			font: `Inter Semi-Bold ${size}`, fontfile, rgba: true, dpi: 72,
			...(width ? { width, wrap: 'word-char' } : {})
		}
	}).png().toBuffer();
	let heading;
	for (let size = 68; size >= 36; size -= 2) {
		heading = await text(title, size, '#eaeaea', 680);
		if ((await sharp(heading).metadata()).height <= 240) break;
	}
	if ((await sharp(heading).metadata()).height > 240) throw new Error(`OG title is too long: ${title}`);
	const background = svg(`
		<rect width="1200" height="630" fill="#272727"/>
		<g transform="translate(64 52) scale(.08)" fill="#eaeaea">${mark}</g>
		<g transform="translate(849 194) scale(.624)" fill="#eaeaea">${mark}</g>
	`);
	return sharp(background).composite([
		{ input: await text('Cydonia', 28), left: 112, top: 55 },
		...(label ? [{ input: await text(label, 18, '#aaaaaa'), left: 64, top: 151 }] : []),
		{ input: heading, left: 60, top: 204 },
		{ input: await text(subtitle, 23, '#aaaaaa'), left: 64, top: 472 },
		{ input: await text('cydonia.sh', 18, '#aaaaaa'), left: 64, top: 564 }
	]).png().toBuffer();
}

export async function generateOg() {
	const releases = published(JSON.parse(await readFile(at('../../changelog.json'), 'utf8')));
	const cards = [
		{ key: 'home', title: 'Where agents\nkeep their work.', label: '', subtitle: 'A desktop workspace for coding agents.' },
		{ key: 'changelog', title: 'Changelog', label: 'Releases', subtitle: 'What’s new. What’s changed.' },
		...releases.map(({ version }) => ({ key: `releases/${version}`, title: `v${version}`, label: 'Release notes', subtitle: 'Where agents keep their work.' })),
		...slugs().map((slug) => ({ key: `docs/${slug}`, title: metadata(slug).title, label: 'Documentation', subtitle: 'A desktop workspace for coding agents.' }))
	];
	const manifest = {};
	const fontfile = await inter();
	for (const card of cards) {
		const output = await renderCard(card, fontfile);
		const path = `/og/${card.key}.${digest(output).slice(0, 16)}.png`;
		await mkdir(dirname(at(`../static${path}`)), { recursive: true });
		await writeFile(at(`../static${path}`), output);
		manifest[card.key] = { path, alt: `Cydonia — ${card.title.replaceAll('\n', ' ')}. ${card.subtitle}` };
		if (card.key === 'home') await writeFile(at('../static/og.png'), output);
	}
	await mkdir(at('../src/lib/generated'), { recursive: true });
	await writeFile(at('../src/lib/generated/og.json'), JSON.stringify(manifest, null, 2) + '\n');
	console.log(`OG images: ${cards.length} cards → static/og/`);
	return manifest;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	await generateOg();
}
