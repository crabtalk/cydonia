import { outline } from '$lib/docs/server.js';

export const prerender = true;

/** The sidebar, read once and handed to every page under it. */
export const load = () => ({ sections: outline() });
