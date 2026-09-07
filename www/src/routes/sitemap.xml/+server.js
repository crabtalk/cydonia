import { site } from '$lib/meta.js';

export const prerender = true;
export const trailingSlash = 'never';

export function GET() {
	const urls = [`${site}/`];
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
