// Writes the contour maps of Cydonia Mensae the home page draws, from the Mars Orbiter
// Laser Altimeter's gridded topography (MOLA MEGDR, 128 px/deg, NASA, public domain):
// static/terrain.svg, the region behind the page, and static/terrain-plate.svg, the
// footer plate's scene: its window plus PLATE_OVERSCAN on every side, centred on the
// Face (40.75°N 350.54°E) to the pixel; and static/terrain-relief.json, the region's
// contours a level each and its heights, which /backdrop/ animates.
// Run with `npm run terrain`; its outputs are committed, so builds never fetch the data.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const TILE = 'https://pds-geosciences.wustl.edu/mgs/mgs-m-mola-5-megdr-l3-v1/mgsl_300x/meg128/megt44n270hb.img';
const TILE_COLS = 11520;
const TILE_LAT = 44;
const TILE_LON = 270;
const PPD = 128;
const BAND_ROW = 240;
const BAND_ROWS = 352;
const FACE = [40.75, 350.54];
const STEP = 40;
const INDEX = 200;
const SIGMA = 1.3;
const SIMPLIFY = 0.25;
const PLATE_ROWS = 228;
const PLATE_COLS = 1024;
const PLATE_OVERSCAN = 0.08;
const FIELD_CELL = 4;

const at = (path) => fileURLToPath(new URL(path, import.meta.url));
const KX = Math.cos((FACE[0] * Math.PI) / 180);
const FACE_ROW = Math.round((TILE_LAT - FACE[0]) * PPD) - BAND_ROW;
const FACE_COL = Math.round((FACE[1] - TILE_LON) * PPD);

/** The tile's rows BAND_ROW.. for BAND_ROWS, fetched with a byte range and cached in .cache/. */
async function band() {
	const path = at('../.cache/mola-cydonia.img');
	const size = BAND_ROWS * TILE_COLS * 2;
	const cached = await readFile(path).catch(() => null);
	if (cached?.length === size) return cached;
	const from = BAND_ROW * TILE_COLS * 2;
	const response = await fetch(TILE, { headers: { Range: `bytes=${from}-${from + size - 1}` }, signal: AbortSignal.timeout(120_000) });
	if (response.status !== 206) throw new Error(`terrain: ${response.status} fetching the MOLA tile`);
	const data = Buffer.from(await response.arrayBuffer());
	if (data.length !== size) throw new Error(`terrain: got ${data.length} bytes, wanted ${size}`);
	await mkdir(at('../.cache/'), { recursive: true });
	await writeFile(path, data);
	return data;
}

function blur(src, w, h, sigma) {
	const n = Math.ceil(sigma * 3);
	const kernel = Array.from({ length: 2 * n + 1 }, (_, i) => Math.exp(-((i - n) ** 2) / (2 * sigma * sigma)));
	const sum = kernel.reduce((a, b) => a + b, 0);
	const pass = (from, index) => {
		const out = new Float32Array(w * h);
		for (let r = 0; r < h; r++)
			for (let c = 0; c < w; c++) {
				let v = 0;
				for (let i = -n; i <= n; i++) v += kernel[i + n] * from[index(r, c, i)];
				out[r * w + c] = v / sum;
			}
		return out;
	};
	const across = pass(src, (r, c, i) => r * w + Math.min(w - 1, Math.max(0, c + i)));
	return pass(across, (r, c, i) => Math.min(h - 1, Math.max(0, r + i)) * w + c);
}

// Marching squares at one level, its segments joined into polylines.
function contour(grid, w, h, level) {
	const value = (r, c) => grid[r * w + c];
	const cut = (a, b, va, vb) => a + ((level - va) / (vb - va)) * (b - a);
	const segments = [];
	for (let r = 0; r < h - 1; r++)
		for (let c = 0; c < w - 1; c++) {
			const a = value(r, c), b = value(r, c + 1), d = value(r + 1, c), e = value(r + 1, c + 1);
			const index = (a > level) | ((b > level) << 1) | ((e > level) << 2) | ((d > level) << 3);
			if (index === 0 || index === 15) continue;
			const top = [cut(c, c + 1, a, b), r];
			const right = [c + 1, cut(r, r + 1, b, e)];
			const bottom = [cut(c, c + 1, d, e), r + 1];
			const left = [c, cut(r, r + 1, a, d)];
			const cases = {
				1: [[left, top]], 2: [[top, right]], 3: [[left, right]], 4: [[right, bottom]],
				5: [[left, top], [right, bottom]], 6: [[top, bottom]], 7: [[left, bottom]], 8: [[bottom, left]],
				9: [[bottom, top]], 10: [[top, right], [bottom, left]], 11: [[bottom, right]], 12: [[right, left]],
				13: [[right, top]], 14: [[top, left]]
			};
			segments.push(...cases[index]);
		}
	const key = ([x, y]) => `${x.toFixed(3)},${y.toFixed(3)}`;
	const ends = new Map();
	segments.forEach((segment, i) => {
		for (const point of segment) {
			const k = key(point);
			if (!ends.has(k)) ends.set(k, []);
			ends.get(k).push(i);
		}
	});
	const used = new Uint8Array(segments.length);
	const lines = [];
	for (let i = 0; i < segments.length; i++) {
		if (used[i]) continue;
		used[i] = 1;
		const line = [...segments[i]];
		for (const forward of [true, false]) {
			for (;;) {
				const tip = forward ? line[line.length - 1] : line[0];
				const next = (ends.get(key(tip)) ?? []).find((j) => !used[j]);
				if (next === undefined) break;
				used[next] = 1;
				const [p, q] = segments[next];
				const other = key(p) === key(tip) ? q : p;
				if (forward) line.push(other);
				else line.unshift(other);
			}
		}
		if (line.length > 3) lines.push(line);
	}
	return lines;
}

// Ramer–Douglas–Peucker.
function simplify(points, epsilon) {
	if (points.length < 3) return points;
	const a = points[0];
	const b = points[points.length - 1];
	const dx = b[0] - a[0];
	const dy = b[1] - a[1];
	const length = Math.hypot(dx, dy) || 1;
	let far = 0;
	let at = 0;
	for (let i = 1; i < points.length - 1; i++) {
		const d = Math.abs(dy * points[i][0] - dx * points[i][1] + b[0] * a[1] - b[1] * a[0]) / length;
		if (d > far) [far, at] = [d, i];
	}
	if (far < epsilon) return [a, b];
	return [...simplify(points.slice(0, at + 1), epsilon).slice(0, -1), ...simplify(points.slice(at), epsilon)];
}

/** The band's rows `row`.. for `h`, tile columns `col`.. for `w`, blurred, and its contours a level each. */
function relief(data, row, h, col, w) {
	let grid = new Float32Array(w * h);
	for (let r = 0; r < h; r++) for (let c = 0; c < w; c++) grid[r * w + c] = data.readInt16BE(((row + r) * TILE_COLS + col + c) * 2);
	grid = blur(grid, w, h, SIGMA);
	let lo = Infinity;
	let hi = -Infinity;
	for (const v of grid) [lo, hi] = [Math.min(lo, v), Math.max(hi, v)];
	const path = (points) => 'M' + points.map(([x, y]) => `${(x * KX).toFixed(1)} ${y.toFixed(1)}`).join('L');
	const levels = [];
	for (let level = Math.ceil(lo / STEP) * STEP; level < hi; level += STEP)
		levels.push({ m: level, d: contour(grid, w, h, level).map((line) => path(simplify(line, SIMPLIFY))).join('') });
	return { grid, w, h, levels };
}

/** A relief's contours as a mask SVG. */
function drawing({ w, h, levels }) {
	const minor = levels.filter(({ m }) => m % INDEX !== 0).map(({ d }) => d);
	const major = levels.filter(({ m }) => m % INDEX === 0).map(({ d }) => d);
	// White strokes: used as a mask, so the minor lines carry half the index lines' weight.
	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${(w * KX).toFixed(0)} ${h}"><g fill="none" stroke="#fff" stroke-width="0.5" stroke-linejoin="round"><path stroke-opacity="0.5" d="${minor.join('')}"/><path d="${major.join('')}"/></g></svg>`;
}

/** A relief for the backdrop: its contours a level each in the SVG's units, and its heights in metres every FIELD_CELL px. */
function heights({ grid, w, h, levels }) {
	const cols = Math.floor(w / FIELD_CELL);
	const rows = Math.floor(h / FIELD_CELL);
	const field = [];
	for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) field.push(Math.round(grid[r * FIELD_CELL * w + c * FIELD_CELL]));
	return JSON.stringify({
		width: Math.round(w * KX),
		height: h,
		index: INDEX,
		levels,
		field: { cols, rows, cell: [+(FIELD_CELL * KX).toFixed(3), FIELD_CELL], heights: field }
	});
}

export async function generateTerrain() {
	const data = await band();
	const area = relief(data, 0, BAND_ROWS, (346.5 - TILE_LON) * PPD, 8 * PPD);
	const region = drawing(area);
	const backdrop = heights(area);
	const rows = 2 * Math.round((PLATE_ROWS * (1 + 2 * PLATE_OVERSCAN)) / 2);
	const cols = 2 * Math.round((PLATE_COLS * (1 + 2 * PLATE_OVERSCAN)) / 2);
	const plate = drawing(relief(data, FACE_ROW - rows / 2, rows, FACE_COL - cols / 2, cols));
	await writeFile(at('../static/terrain.svg'), region);
	await writeFile(at('../static/terrain-plate.svg'), plate);
	await writeFile(at('../static/terrain-relief.json'), backdrop);
	console.log(`terrain: region ${(region.length / 1024).toFixed(0)} KB, plate ${(plate.length / 1024).toFixed(0)} KB, relief ${(backdrop.length / 1024).toFixed(0)} KB`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await generateTerrain();
