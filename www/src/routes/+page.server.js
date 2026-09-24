import { highlight } from '$lib/files.js';

export const load = async () => ({ files: await highlight() });
