import { error } from '@sveltejs/kit';
import { releases } from '$lib/changelog.js';

export const entries = () => releases.map(({ version }) => ({ version }));

export function load({ params }) {
	const release = releases.find(({ version }) => version === params.version);
	if (!release) error(404, 'No such release');
	return { release };
}
