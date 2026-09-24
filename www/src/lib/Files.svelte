<script>
	import { Braces, Database, File, FileText, FolderOpen } from 'lucide-static';

	/** Each file carries `path` and `lines`: token rows from `highlight`. */
	let { files } = $props();

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

	const file = $derived(files[selected]);
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
			<span class="size">{file.lines.length} lines</span>
		</figcaption>
		<pre class="shiki"><code>{#each file.lines as line, i}<span class="line"><span class="n" aria-hidden="true">{i + 1}</span><span>{#each line as token}<span style={token.style}>{token.content}</span>{:else}{' '}{/each}</span></span>{/each}</code></pre>
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
