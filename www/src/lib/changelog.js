// One file at the repo root is the whole history, the same shape bezel keeps:
// a version, the day it shipped, a sentence, and the three change groups.
import entries from '../../../changelog.json';

/** Newest first. The release that is out is the one at the top. */
export const releases = entries;

export const latest = entries[0];

/** Only the groups a release actually filled, in the order notes read them. */
export const groups = (release) =>
	[
		{ title: 'New', items: release.new ?? [] },
		{ title: 'Changed', items: release.changed ?? [] },
		{ title: 'Fixed', items: release.fixed ?? [] }
	].filter((group) => group.items.length);

// Pinned to UTC: these are calendar days, and a westward zone would render
// `2026-09-07` as the sixth.
export const day = (iso) =>
	new Date(iso).toLocaleDateString('en-US', {
		year: 'numeric',
		month: 'short',
		day: 'numeric',
		timeZone: 'UTC'
	});

export const anchor = (version) => `v${version}`;
