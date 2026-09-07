import adapter from '@sveltejs/adapter-static';

// No `paths.base` on purpose: SvelteKit emits relative links by default, so the
// same build serves from the domain and from a local preview without a prefix
// to keep in sync.
export default {
	kit: { adapter: adapter({ fallback: '404.html' }) }
};
