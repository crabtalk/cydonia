/** Shared by the site and the build-time share image generator. */
export const cdn = 'https://cdn.crabtalk.ai';

const VIDEO = /\.(mp4|webm|mov)$/i;

/** A release may carry one clip or still:
 *
 *     "media": { "src": "videos/0.1.1.mp4", "poster": "pics/0.1.1.jpg",
 *                "alt": "…", "w": 1280, "h": 804 }
 *
 * `src` and `poster` are paths under the CDN, though a full URL is passed
 * through — this file is edited by hand and both spellings are reasonable to
 * reach for. A release with only a still gives `poster` and no `src`, and the
 * still is what renders. `w`/`h` are optional and fix the box so the page does
 * not reflow when the file arrives. Returns null when a release carries
 * nothing, which is most of them.
 */
export const media = (release) => {
	const m = release.media;
	if (!m) return null;

	const at = (path) => (!path || /^https?:\/\//.test(path) ? path : `${cdn}/${path}`);
	const rest = { alt: m.alt ?? '', w: m.w, h: m.h };

	// No clip was made for this one, so the still is the media rather than the
	// thing standing in front of it.
	if (!m.src) return m.poster ? { src: at(m.poster), video: false, ...rest } : null;

	return { src: at(m.src), poster: at(m.poster), video: VIDEO.test(m.src), ...rest };
};
