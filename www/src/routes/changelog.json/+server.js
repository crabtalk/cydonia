import { releases } from '$lib/changelog.js';

// Prerendered like the sitemap, so the feed is a file on the CDN rather than a
// function: the app reads it on launch, and that should cost nobody a cold
// start. `vite.config.js` already refuses to build when the head of the
// changelog is not the version in `Cargo.toml`, which is what keeps this from
// announcing a release that was never cut.
export const prerender = true;
export const trailingSlash = 'never';

/** The updater's feed — the same history the changelog page renders.
 *
 *  Only `version` is load-bearing: the app derives the dmg's address from it
 *  the way `meta.js` does for the download button, so a release adds nothing
 *  here beyond its entry. The rest rides along because it is already written,
 *  and is what a "what's new" pane would read.
 */
export function GET() {
	return new Response(`${JSON.stringify(releases)}\n`, {
		headers: { 'content-type': 'application/json; charset=utf-8' }
	});
}
