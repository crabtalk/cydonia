import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sveltekit } from '@sveltejs/kit/vite';
import { generateOg } from './scripts/generate-og.mjs';

const at = (path) => fileURLToPath(new URL(path, import.meta.url));

// The app and the site ship from one repo and one version: the git tag is `v`
// and the crate version, the dmg is named after it too. Reading Cargo.toml here
// is why the download link needs no API call and cannot drift from the release.
const cargo = readFileSync(at('../Cargo.toml'), 'utf8');
const version = cargo.match(/^version = "(.+)"/m)?.[1];
if (!version) throw new Error('no package version in ../Cargo.toml');

// The changelog leads with the release that is out. Bumping the crate without
// writing the entry would ship a page offering a dmg it never names, so the
// build refuses rather than deploying the mismatch.
const entries = JSON.parse(readFileSync(at('../changelog.json'), 'utf8'));
const [newest] = entries;
if (newest?.version !== version) {
	throw new Error(
		`changelog.json leads with ${newest?.version}, Cargo.toml is ${version} — add the entry`
	);
}

// `define` and the guard above are resolved once, when this config is
// evaluated. Vite watches the config file but cannot know it read these two, so
// a dev server started before a version bump keeps serving the old one.
let images;
const release = {
	name: 'watch-release-inputs',
	async configResolved(config) {
		// The manifest must exist before either SSR or client compilation.
		if (!config.isPreview) await (images ??= generateOg());
	},
	configureServer(server) {
		const docs = resolve(at('../docs'));
		const watched = [resolve(at('../Cargo.toml')), resolve(at('../changelog.json')), docs, resolve(at('./scripts/generate-og.mjs'))];
		server.watcher.add(watched);
		server.watcher.on('all', (event, file) => {
			if (!['add', 'change', 'unlink'].includes(event)) return;
			if (watched.includes(resolve(file)) || resolve(file).startsWith(`${docs}/`)) server.restart();
		});
	}
};

export default {
	plugins: [sveltekit(), release],
	// Prefixed, because `define` rewrites the token everywhere, dependencies included.
	define: { __CYDONIA_VERSION__: JSON.stringify(version) },
	// The README is served as itself at `/readme.md`, and it lives a directory
	// above: package.json is in here, so Vite roots the dev server at www.
	server: { fs: { allow: ['..'] } }
};
