/** Shared by the site and the build-time share image generator. */
export const cdn = 'https://cdn.crabtalk.ai';

const VIDEO = /\.(mp4|webm|mov)$/i;

/** Omitted media uses versioned CDN paths. Explicit poster-only media is a
 * still image; null disables media. Paths may also be absolute URLs.
 */
export const media = (release) => {
	const m = release.media === undefined
		? {
			src: `videos/cydonia/v${release.version}.mp4`,
			poster: `pics/cydonia/v${release.version}.jpg`,
			w: 1280,
			h: 900
		}
		: release.media;
	if (!m) return null;

	const at = (path) => (!path || /^https?:\/\//.test(path) ? path : `${cdn}/${path}`);
	const rest = { alt: m.alt ?? '', w: m.w, h: m.h };

	// No clip was made for this one, so the still is the media rather than the
	// thing standing in front of it.
	if (!m.src) return m.poster ? { src: at(m.poster), video: false, ...rest } : null;

	return { src: at(m.src), poster: at(m.poster), video: VIDEO.test(m.src), ...rest };
};
