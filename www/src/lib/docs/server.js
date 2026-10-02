import { readFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import matter from 'gray-matter';
import rehypeAutolinkHeadings from 'rehype-autolink-headings';
import rehypeShiki from '@shikijs/rehype';
import rehypeSlug from 'rehype-slug';
import rehypeStringify from 'rehype-stringify';
import remarkGfm from 'remark-gfm';
import remarkParse from 'remark-parse';
import remarkRehype from 'remark-rehype';
import { unified } from 'unified';
import { visit } from 'unist-util-visit';
import { href } from './nav.js';
import { slugs, dir } from './catalog.js';
export { slugs } from './catalog.js';

/** The docs live in `docs/` at the root of the repository, beside the crates
    they describe rather than inside the site. Read with `fs` because the
    directory is outside Vite's root, and read at build time only: every page
    under `/docs` is prerendered, so none of this reaches the client. */
const source = (slug) => readFileSync(resolve(dir, `${slug}.md`), 'utf8');


/** The table of contents, read from `SUMMARY.md`: a `# Part` heading opens a
    section, and `- [Title](./slug.md)` is a page in it. A link before the
    first heading belongs to no section — that is the index. */
export function outline() {
	const written = slugs();
	const sections = [];
	let section = null;
	for (const line of source('SUMMARY').split('\n')) {
		const part = line.match(/^#+\s+(.+)$/);
		if (part) {
			if (part[1] === 'Summary') continue;
			section = { title: part[1], pages: [] };
			sections.push(section);
			continue;
		}
		const entry = line.match(/^\s*(?:-\s*)?\[(.+?)\]\((.+?)\.md\)/);
		if (!entry) continue;
		const slug = entry[2].replace(/^\.\//, '');
		// Loudly, at build: a summary naming a page nobody wrote is a dead
		// sidebar row, and it would otherwise ship.
		if (!written.includes(slug)) {
			throw new Error(`docs: SUMMARY.md names a missing page "${slug}" (docs/${slug}.md)`);
		}
		if (!section) {
			section = { title: '', pages: [] };
			sections.push(section);
		}
		section.pages.push({ title: entry[1], slug });
	}
	return sections;
}

/** A page links to its neighbour by relative path — `../agents/mcp.md`, which
    is what GitHub resolves when the directory is read there. The site serves
    the same page at `/docs/agents/mcp/`, so the path is resolved against the
    linking page and handed back as a slug. */
const relink = (html, slug) =>
	html.replace(/href="([^"]+?)\.md(#[^"]*)?"/g, (whole, path, hash) => {
		if (/^[a-z]+:/.test(path)) return whole;
		const target = relative(dir, resolve(dir, dirname(slug), path));
		return `href="${href(target)}${hash ?? ''}"`;
	});

/** Collect the headings a page can be linked to, for the column beside it.
    Runs before the autolink wrap, while the heading text is still plain. */
function contents(acc) {
	return (tree) => {
		visit(tree, 'element', (node) => {
			if (node.tagName !== 'h2' && node.tagName !== 'h3') return;
			const id = node.properties?.id;
			if (typeof id !== 'string') return;
			let text = '';
			visit(node, 'text', (leaf) => {
				text += leaf.value;
			});
			acc.push({ depth: node.tagName === 'h2' ? 2 : 3, id, text: text.trim() });
		});
	};
}

/** `<!-- include ../../crates/prompts/resources/markdown.md -->` on a line of
    its own becomes that file, frontmatter dropped, in a fence — the text an
    agent is handed, verbatim. The path is relative to the including page.
    GitHub shows nothing for the comment. */
const include = (content, slug) =>
	content.replace(/^<!-- include (\S+) -->$/gm, (_, path) => {
		const text = matter(readFileSync(resolve(dir, dirname(slug), path), 'utf8')).content;
		return '````markdown\n' + text.trim() + '\n````';
	});

/** A page's frontmatter, rendered body and headings, or nothing where the slug
    names no file. */
export async function read(slug) {
	let raw;
	try {
		raw = source(slug);
	} catch {
		return null;
	}
	const { data, content } = matter(raw);
	const toc = [];
	const file = await unified()
		.use(remarkParse)
		.use(remarkGfm)
		.use(remarkRehype)
		.use(rehypeSlug)
		.use(contents, toc)
		.use(rehypeAutolinkHeadings, { behavior: 'wrap' })
		.use(rehypeShiki, {
			themes: { light: 'github-light', dark: 'github-dark' },
			defaultColor: false,
			// A fence with no language is skipped outright without this pair.
			defaultLanguage: 'text',
			fallbackLanguage: 'text'
		})
		.use(rehypeStringify)
		.process(include(content, slug));

	return {
		slug,
		title: data.title ?? slug,
		description: data.description ?? '',
		toc,
		html: relink(String(file), slug)
	};
}
