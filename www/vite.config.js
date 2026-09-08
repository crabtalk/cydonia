import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { sveltekit } from '@sveltejs/kit/vite';

// The app and the site ship from one repo and one version: the git tag is `v`
// and the crate version, the dmg is named after it too. Reading Cargo.toml here
// is why the download link needs no API call and cannot drift from the release.
const cargo = readFileSync(fileURLToPath(new URL('../Cargo.toml', import.meta.url)), 'utf8');
const version = cargo.match(/^version = "(.+)"/m)?.[1];
if (!version) throw new Error('no package version in ../Cargo.toml');

export default {
	plugins: [sveltekit()],
	// Prefixed, because `define` rewrites the token everywhere, dependencies included.
	define: { __CYDONIA_VERSION__: JSON.stringify(version) },
	// The README is served as itself at `/readme.md`, and it lives a directory
	// above: package.json is in here, so Vite roots the dev server at www.
	server: { fs: { allow: ['..'] } }
};
