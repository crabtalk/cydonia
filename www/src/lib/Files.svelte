<script>
	import { Braces, Database, File, FileText, FolderOpen } from 'lucide-static';

	// A made-up project after one agent session: the plan it wrote, the board
	// it filled and the transcript. Shapes follow what cydonia writes to disk,
	// trimmed to fit.
	const files = [
		{
			path: 'articles/1790089015001/content.md',
			text: `## Why now

The public API has no limit. One client replayed a queue
last Tuesday and took p99 from 80ms to 4s for everyone.

## Plan

- Token bucket per API key, kept in Redis
- 600 requests a minute by default, raised per plan
- Answer 429 with \`Retry-After\`, never drop silently

## Open

- Do webhooks count against the same bucket?`
		},
		{
			path: 'articles/1790089015001/properties.toml',
			text: `title = "Rate limiting plan"
archived = false`
		},
		{
			path: 'boards/1790266971001.toml',
			text: `name = "Roadmap"
key = "ROAD"
next_handle = 4

[[columns]]
name = "TODO"

[[columns.cards]]
handle = 1
text = "Token bucket middleware"

[[columns.cards]]
handle = 2
text = "429 with Retry-After"

[[columns.cards]]
handle = 3
text = "Decide: webhooks and the bucket"`
		},
		{
			path: 'sessions/1790252287183.json',
			text: `{
  "agent": "Claude Agent",
  "title": "Plan rate limiting",
  "closed": true,
  "items": [
    { "User": "Draft a rate limiting plan and put the work on ROAD" },
    { "Tool": "article_add Rate limiting plan" },
    { "Tool": "board_add_card ROAD ×3" },
    { "Agent": "The plan is in the article; three cards are on ROAD." }
  ]
}`
		}
	];

	let selected = $state(0);

	const icons = { md: FileText, toml: File, json: Braces };
	const iconFor = (name) => icons[name.split('.').pop()] ?? File;

	// Directory rows come from the paths so the tree cannot disagree with them.
	const rows = $derived.by(() => {
		const out = [];
		const seen = new Set();
		files.forEach((file, index) => {
			const parts = file.path.split('/');
			parts.forEach((name, depth) => {
				const key = parts.slice(0, depth + 1).join('/');
				if (seen.has(key)) return;
				seen.add(key);
				const leaf = depth === parts.length - 1;
				out.push({ key, name, depth, index: leaf ? index : -1 });
			});
		});
		return out;
	});

	const escape = (text) =>
		text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');

	// Headings bold and structure faint; everything else stays plain.
	function paint(line, kind) {
		const text = escape(line);
		if (kind === 'md') {
			if (/^#+ /.test(text)) return `<span class="t-head">${text}</span>`;
			return text.replace(/^- /, '<span class="t-punct">- </span>');
		}
		if (kind === 'toml' && /^\[/.test(text)) return `<span class="t-punct">${text}</span>`;
		return text;
	}

	const file = $derived(files[selected]);
	const lines = $derived.by(() => {
		const kind = file.path.split('.').pop();
		return file.text.split('\n').map((line) => paint(line, kind));
	});
</script>

<div class="files">
	<nav class="tree" aria-label="Files in .cydonia">
		<p class="root"><span aria-hidden="true">{@html FolderOpen}</span>.cydonia</p>
		<ul>
			{#each rows as row (row.key)}
				<li>
					{#if row.index < 0}
						<span class="row dir" style="--depth: {row.depth}">
							<span aria-hidden="true">{@html FolderOpen}</span>{row.name}
						</span>
					{:else}
						<button type="button" class="row" style="--depth: {row.depth}"
							aria-current={selected === row.index} onclick={() => (selected = row.index)}>
							<span aria-hidden="true">{@html iconFor(row.name)}</span>{row.name}
						</button>
					{/if}
				</li>
			{/each}
			<li>
				<span class="row dir" style="--depth: 0">
					<span aria-hidden="true">{@html Database}</span>entries.db
				</span>
			</li>
		</ul>
	</nav>
	<figure class="view">
		<figcaption>
			<span class="where">.cydonia/{file.path}</span>
			<span class="size">{lines.length} lines</span>
		</figcaption>
		<pre><code>{#each lines as line, i}<span class="line"><span class="n" aria-hidden="true">{i + 1}</span><span>{@html line || ' '}</span></span>{/each}</code></pre>
	</figure>
</div>

<style>
	.files {
		display: grid;
		grid-template-columns: 232px minmax(0, 1fr);
		margin: 32px 0 0;
		border: 1px solid var(--line);
		overflow: hidden;
		font-family: var(--mono);
		font-size: 13px;
	}

	:global(.files svg) {
		width: 14px;
		height: 14px;
		flex: none;
	}

	.tree {
		padding: 12px 0;
		border-right: 1px solid var(--line);
	}

	.root,
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 28px;
		white-space: nowrap;
	}

	.root {
		margin: 0;
		padding: 0 16px;
		color: var(--text);
	}

	.root span,
	.row span {
		display: inline-flex;
		color: var(--faint);
	}

	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.row {
		width: 100%;
		padding: 0 16px 0 calc(16px + (var(--depth) + 1) * 14px);
		border: 0;
		background: transparent;
		color: var(--muted);
		font: inherit;
		text-align: left;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	button.row {
		cursor: pointer;
	}

	button.row:hover {
		background: var(--panel-high);
		color: var(--text);
	}

	button.row[aria-current='true'] {
		background: var(--text);
		color: var(--bg);
	}

	button.row[aria-current='true'] span {
		color: var(--bg);
	}

	.dir {
		color: var(--faint);
	}

	button.row:focus-visible {
		outline: 2px solid var(--text);
		outline-offset: -2px;
	}

	.view {
		min-width: 0;
		margin: 0;
		background: var(--bg);
	}

	figcaption {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		height: 40px;
		padding: 0 20px;
		border-bottom: 1px solid var(--line);
		font-size: 12px;
	}

	.where {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text);
	}

	.size {
		flex: none;
		color: var(--faint);
	}

	pre {
		height: 360px;
		margin: 0;
		padding: 16px 0;
		border: 0;
		background: none;
		overflow: auto;
		line-height: 1.7;
	}

	code {
		display: grid;
		background: none;
		color: var(--text);
	}

	.line {
		display: flex;
		padding-right: 20px;
	}

	.n {
		flex: none;
		width: 48px;
		padding-right: 16px;
		text-align: right;
		color: var(--faint);
		opacity: 0.6;
		user-select: none;
	}

	.line :global(.t-head) {
		font-weight: 600;
	}

	.line :global(.t-punct) {
		color: var(--faint);
	}

	@media (max-width: 720px) {
		.files {
			grid-template-columns: minmax(0, 1fr);
		}

		.tree {
			border-right: 0;
			border-bottom: 1px solid var(--line);
		}

		.size {
			display: none;
		}
	}
</style>
