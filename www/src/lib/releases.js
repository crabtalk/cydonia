/** Published entries, preserving the changelog's newest-first order. */
export const published = (entries) => entries.filter((entry) => !entry.nightly);

export const releasePath = (version) => `/changelog/${version}/`;
