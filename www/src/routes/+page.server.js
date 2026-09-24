import { highlight } from '$lib/files.js';

export const load = async () => ({ roots: await highlight() });
