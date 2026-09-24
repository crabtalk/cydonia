import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import sharp from 'sharp';
import { media } from '../src/lib/media.js';

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

/** Fetch the newest release's still. A missing poster must not silently ship
 * an old release's image. Upload the media before building the site. */
export async function generateOg(latest) {
	const shot = media(latest);
	const source = shot?.video ? shot.poster : shot?.src;
	if (!source) throw new Error(`OG image: v${latest.version} needs a screenshot or video poster`);
	const response = await fetch(source, { signal: AbortSignal.timeout(30_000) });
	if (!response.ok) throw new Error(`OG image: ${response.status} fetching ${source}`);
	const input = Buffer.from(await response.arrayBuffer());
	const { width, height } = await sharp(input).metadata();
	if (!width || !height) throw new Error(`OG image: invalid screenshot at ${source}`);

	// Show the top half at full card width, preserving its proportions. The
	// window continues below the card rather than squeezing a whole desktop in.
	const screenshot = await sharp(input)
		.extract({ left: 0, top: 0, width, height: Math.ceil(height / 2) })
		.resize(1080, 352, { fit: 'cover', position: 'north' })
		.composite([{ input: svg('<rect width="1080" height="704" rx="16" fill="white"/>', 1080, 352), blend: 'dest-in' }])
		.png().toBuffer();

	const fontfile = await inter();
	const text = async (value, size, weight = '', color = '#0b0b0c') => sharp({
		text: {
			text: `<span foreground="${color}">${escape(value)}</span>`,
			font: `Inter ${weight} ${size}`,
			fontfile,
			rgba: true,
			dpi: 72
		}
	}).png().toBuffer();
	const background = svg(`
		<rect width="1200" height="630" fill="#f7f7f5"/>
		<g transform="translate(60 46) scale(.075)" fill="#0b0b0c">
			<path d="M79 98 404 0 316 185Z"/><path d="M162 166 365 261 0 393Z"/>
		</g>
		<rect x="48" y="266" width="1104" height="780" rx="28" fill="#e9e9e6" stroke="#dededb"/>
	`);
	const version = await text(`v${latest.version}`, 20, '', '#737373');
	const versionWidth = (await sharp(version).metadata()).width;
	const output = await sharp(background).composite([
		{ input: await text('Cydonia', 28, 'Semi-Bold'), left: 103, top: 47 },
		{ input: version, left: 1140 - versionWidth, top: 53 },
		{ input: await text('Where agents keep', 56, 'Semi-Bold'), left: 60, top: 117 },
		{ input: await text('their work.', 56, 'Semi-Bold'), left: 60, top: 182 },
		{ input: screenshot, left: 60, top: 278 }
	]).png().toBuffer();
	await writeFile(at('../static/og.png'), output);
	console.log(`OG image: v${latest.version} → static/og.png`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const [latest] = JSON.parse(await readFile(at('../../changelog.json'), 'utf8'));
	await generateOg(latest);
}
