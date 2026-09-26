import { site } from '$lib/meta.js';
import { releases, releasePath } from '$lib/changelog.js';
import { slugs } from '$lib/docs/catalog.js';
import { href } from '$lib/docs/nav.js';

export const prerender = true;
export const trailingSlash = 'never';

export function GET() {
	const paths = ['/', '/changelog/', ...releases.map(({ version }) => releasePath(version)), ...slugs().map(href)];
	const urls = paths.map((path) => `${site}${path}`);
	const xml = [
		'<?xml version="1.0" encoding="UTF-8"?>',
		'<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">',
		...urls.map((loc) => `\t<url><loc>${loc}</loc></url>`),
		'</urlset>'
	].join('\n');

	return new Response(`${xml}\n`, {
		headers: { 'content-type': 'application/xml; charset=utf-8' }
	});
}
