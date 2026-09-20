/** The path a page is served at. `index` is `/docs/` itself. */
export const href = (slug) => (slug === 'index' ? '/docs/' : `/docs/${slug}/`);

/** What comes before and after `slug` in the table of contents. */
export function neighbours(sections, slug) {
	const pages = sections.flatMap((section) => section.pages);
	const index = pages.findIndex((page) => page.slug === slug);
	return { prev: pages[index - 1], next: pages[index + 1] };
}
