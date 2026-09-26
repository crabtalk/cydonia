import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import test from 'node:test';
import sharp from 'sharp';
import { published, releasePath } from '../src/lib/releases.js';

const json = async (path) => JSON.parse(await readFile(new URL(path, import.meta.url), 'utf8'));
const manifest = await json('../src/lib/generated/og.json');
const history = await json('../../changelog.json');
const releases = published(history);

const pagePath = (key) => key === 'home' ? '/' : key === 'docs/index' ? '/docs/'
	: key.startsWith('releases/') ? releasePath(key.slice(9)) : `/${key}/`;

test('every card is served in its prerendered page with matching content hash', async () => {
	for (const [key, image] of Object.entries(manifest)) {
		const html = await readFile(new URL(`../build${pagePath(key)}index.html`, import.meta.url), 'utf8');
		const og = html.match(/<meta property="og:image" content="([^"]+)"/g);
		assert.equal(og?.length, 1, key);
		assert.ok(og[0].includes(`https://cydonia.sh${image.path}`), key);
		assert.ok(html.includes(`<meta name="twitter:image" content="https://cydonia.sh${image.path}"`), key);
		assert.ok(html.includes(`<link rel="canonical" href="https://cydonia.sh${pagePath(key)}"`), key);
		const png = await readFile(new URL(`../build${image.path}`, import.meta.url));
		const hash = createHash('sha256').update(png).digest('hex').slice(0, 16);
		assert.ok(image.path.endsWith(`.${hash}.png`), key);
		const { width, height } = await sharp(png).metadata();
		assert.deepEqual([width, height], [1200, 630], key);
	}
});

test('released versions have distinct cards, routes and sitemap entries', async () => {
	const sitemap = await readFile(new URL('../build/sitemap.xml', import.meta.url), 'utf8');
	const index = await readFile(new URL('../build/changelog/index.html', import.meta.url), 'utf8');
	for (const { version } of releases) {
		assert.ok(manifest[`releases/${version}`]);
		assert.ok(sitemap.includes(`https://cydonia.sh${releasePath(version)}`));
		assert.ok(index.includes(`id="v${version}"`));
	}
	assert.equal(new Set(releases.map(({ version }) => manifest[`releases/${version}`].path)).size, releases.length);
	const { load } = await import('../.svelte-kit/output/server/entries/pages/changelog/_version_/_page.server.js');
	assert.throws(() => load({ params: { version: 'not-a-release' } }), (error) => error.status === 404);
});

test('nightly entries are excluded without relying on their position', () => {
	const entries = [{ version: '3', nightly: true }, { version: '2' }, { version: '1', nightly: true }, { version: '0', nightly: false }];
	assert.deepEqual(published(entries).map(({ version }) => version), ['2', '0']);
});
