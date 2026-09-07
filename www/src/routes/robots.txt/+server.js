import { site } from '$lib/meta.js';

export const prerender = true;
export const trailingSlash = 'never';

export function GET() {
	const text = ['User-agent: *', 'Allow: /', '', `Sitemap: ${site}/sitemap.xml`].join('\n');

	return new Response(`${text}\n`, {
		headers: { 'content-type': 'text/plain; charset=utf-8' }
	});
}
