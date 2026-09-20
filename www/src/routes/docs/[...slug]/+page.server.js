import { error } from '@sveltejs/kit';
import { outline, read, slugs } from '$lib/docs/server.js';
import { neighbours } from '$lib/docs/nav.js';

export const prerender = true;

/** Every page in `docs/`, because the route is a parameter and the crawler
    only follows what something links to. */
export const entries = () => slugs().map((slug) => ({ slug: slug === 'index' ? '' : slug }));

export async function load({ params }) {
	const slug = params.slug ? params.slug.replace(/\/$/, '') : 'index';
	const page = await read(slug);
	if (!page) error(404, 'No such page');
	return { ...page, ...neighbours(outline(), slug) };
}
