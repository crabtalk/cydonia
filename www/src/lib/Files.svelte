<script>
	import { Braces, ChevronDown, Database, File, FileText, Folder, FolderOpen } from 'lucide-static';
	import { SvelteSet } from 'svelte/reactivity';

	/** Each root carries `name`, `extra` and `files`; each file `path`,
	    `lines` as token rows from `highlight`, and an optional `note`. */
	let { roots } = $props();

	let current = $state(0);
	let selected = $state(0);
	let open = $state(false);
	let switcher = $state();

	const root = $derived(roots[current]);
	const file = $derived(root.files[selected]);

	// Directories this deep start collapsed: `boards/<id>/cards/` and below.
	const FOLDED_DEPTH = 2;

	const foldedOf = (root) => {
		const out = new Set();
		for (const { path } of root.files) {
			const parts = path.split('/');
			for (let depth = FOLDED_DEPTH; depth < parts.length - 1; depth++) {
				out.add(parts.slice(0, depth + 1).join('/'));
			}
		}
		return out;
	};

	const folded = new SvelteSet(foldedOf(roots[0]));

	function pick(index) {
		current = index;
		selected = 0;
		open = false;
		folded.clear();
		for (const key of foldedOf(roots[index])) folded.add(key);
	}

	function toggle(key) {
		if (folded.has(key)) folded.delete(key);
		else folded.add(key);
	}

	const icons = { md: FileText, toml: File, json: Braces, db: Database };
	const iconFor = (name) => (name.endsWith('/') ? Folder : (icons[name.split('.').pop()] ?? File));

	// Directory rows come from the paths so the tree cannot disagree with them.
	const rows = $derived.by(() => {
		const out = [];
		const seen = new Set();
		root.files.forEach((file, index) => {
			const parts = file.path.split('/');
			parts.forEach((name, depth) => {
				const key = parts.slice(0, depth + 1).join('/');
				if (seen.has(key)) return;
				seen.add(key);
				const leaf = depth === parts.length - 1;
				out.push({ key, name, depth, index: leaf ? index : -1 });
			});
		});
		// A row under a folded directory is hidden; the directory itself shows.
		return out.filter((row) => {
			const parts = row.key.split('/');
			return !parts.slice(0, -1).some((_, i) => folded.has(parts.slice(0, i + 1).join('/')));
		});
	});
</script>

<svelte:window
	onpointerdown={(event) => {
		if (open && !switcher.contains(event.target)) open = false;
	}}
	onkeydown={(event) => {
		if (open && event.key === 'Escape') open = false;
	}}
/>

<div class="files">
	<nav class="tree" aria-label="Files in {root.name}">
		<div class="switch" bind:this={switcher}>
			<button type="button" class="root" title={root.name} aria-expanded={open}
				onclick={() => (open = !open)}>
				<span aria-hidden="true">{@html FolderOpen}</span><span class="label">{root.name}</span>
				<span class="chevron" aria-hidden="true">{@html ChevronDown}</span>
			</button>
			{#if open}
				<ul class="menu">
					{#each roots as other, i (other.name)}
						<li>
							<button type="button" class="row" style="--depth: -1"
								aria-current={i === current} onclick={() => pick(i)}>
								<span aria-hidden="true">{@html Folder}</span>{other.name}
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
		<ul>
			{#each rows as row (row.key)}
				<li>
					{#if row.index < 0}
						<button type="button" class="row dir" style="--depth: {row.depth}"
							aria-expanded={!folded.has(row.key)} onclick={() => toggle(row.key)}>
							<span aria-hidden="true">{@html folded.has(row.key) ? Folder : FolderOpen}</span>{row.name}
						</button>
					{:else}
						<button type="button" class="row" style="--depth: {row.depth}"
							aria-current={selected === row.index} onclick={() => (selected = row.index)}>
							<span aria-hidden="true">{@html iconFor(row.name)}</span>{row.name}
						</button>
					{/if}
				</li>
			{/each}
			{#each root.extra as name (name)}
				<li>
					<span class="row dir" style="--depth: 0">
						<span aria-hidden="true">{@html iconFor(name)}</span>{name}
					</span>
				</li>
			{/each}
		</ul>
	</nav>
	<figure class="view">
		<figcaption>
			<span class="where">{root.name}/{file.path}</span>
			<span class="size">{file.note ?? `${file.lines.length} lines`}</span>
		</figcaption>
		<pre class="shiki"><code>{#each file.lines as line, i}<span class="line"><span class="n" aria-hidden="true">{i + 1}</span><span>{#each line as token}<span style={token.style}>{token.content}</span>{:else}{' '}{/each}</span></span>{/each}</code></pre>
	</figure>
</div>

<style>
	.files {
		display: grid;
		grid-template-columns: 264px minmax(0, 1fr);
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

	/* The viewer sets the row's height; the tree fills it and scrolls past it. */
	.tree {
		display: flex;
		flex-direction: column;
		height: 0;
		min-height: 100%;
		border-right: 1px solid var(--line);
	}

	.tree > ul {
		flex: 1;
		min-height: 0;
		padding-bottom: 12px;
		overflow-y: auto;
	}

	.root,
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 28px;
		white-space: nowrap;
	}

	.switch {
		position: relative;
		margin-bottom: 8px;
		border-bottom: 1px solid var(--line);
	}

	.root {
		width: 100%;
		height: 40px;
		margin: 0;
		padding: 0 16px;
		border: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		cursor: pointer;
	}

	.root:hover {
		background: var(--panel-high);
	}

	.root .label {
		display: block;
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		color: inherit;
		text-align: left;
	}

	.root .chevron {
		flex: none;
	}

	.root[aria-expanded='true'] .chevron {
		transform: rotate(180deg);
	}

	.menu {
		position: absolute;
		top: 100%;
		left: 0;
		right: 0;
		z-index: 1;
		padding: 4px 0;
		border-bottom: 1px solid var(--line);
		background: var(--bg);
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

	.root:focus-visible,
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
			height: auto;
			border-right: 0;
			border-bottom: 1px solid var(--line);
		}

		.size {
			display: none;
		}
	}
</style>
