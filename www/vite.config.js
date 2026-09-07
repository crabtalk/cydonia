import { sveltekit } from '@sveltejs/kit/vite';

export default {
	plugins: [sveltekit()],
	// The README is served as itself at `/readme.md`, and it lives a directory
	// above: package.json is in here, so Vite roots the dev server at www.
	server: { fs: { allow: ['..'] } }
};
