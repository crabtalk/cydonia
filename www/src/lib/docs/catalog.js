import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import matter from 'gray-matter';

export const dir = resolve(process.cwd(), '../docs');

export function metadata(slug) {
	const { data } = matter(readFileSync(resolve(dir, `${slug}.md`), 'utf8'));
	return { title: data.title ?? slug, description: data.description ?? '' };
}

/** All doc pages, including ones not linked from the sidebar. */
export function slugs(group = '') {
	return readdirSync(resolve(dir, group), { withFileTypes: true }).flatMap((entry) => {
		const path = group ? `${group}/${entry.name}` : entry.name;
		if (entry.isDirectory()) return slugs(path);
		if (!entry.name.endsWith('.md') || entry.name === 'SUMMARY.md') return [];
		return [path.replace(/\.md$/, '')];
	});
}

