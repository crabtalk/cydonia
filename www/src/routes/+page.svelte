<script>
	import { siApple, siGithub, siX } from 'simple-icons';
	import { Check, Copy } from 'lucide-static';
	import Brand from '$lib/Brand.svelte';
	import Frame from '$lib/Frame.svelte';
	import { install, repo, site, tagline as description } from '$lib/meta.js';

	const author = 'https://x.com/tianyi_gc';
	const acp = 'https://agentclientprotocol.com';

	// Off until there are real screenshots to put in the frames — three empty
	// boxes in a row read as an unfinished page. Flip to true to bring it back.
	const showcase = false;

	const scenes = [
		{
			id: 'open',
			title: 'Open a directory',
			body: 'Any folder becomes a project. What you write lands in <code>.cydonia/</code> inside it, gitignored.'
		},
		{
			id: 'write',
			title: 'Write it down',
			body: 'Articles with covers and highlighted code. Boards and tables when a thought wants columns.'
		},
		{
			id: 'hand',
			title: 'Hand it over',
			body: 'Any agent that speaks <a href="' +
				acp +
				'">ACP</a> works in the project. Its edits land in the window you were writing in.'
		}
	];

	const paths = [
		['<project>/.cydonia/', 'articles, boards, sessions'],
		['~/.config/cydonia/', 'settings, MCP servers, agents'],
		['~/.local/share/', 'installed agents']
	];

	// The outline follows the reel: whichever scene owns the middle of the
	// viewport is the one it marks.
	let active = $state(0);
	let nodes = $state([]);

	$effect(() => {
		const observer = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) {
					if (entry.isIntersecting) active = nodes.indexOf(entry.target);
				}
			},
			{ rootMargin: '-45% 0px -45% 0px' }
		);
		for (const node of nodes) if (node) observer.observe(node);
		return () => observer.disconnect();
	});

	const jsonLd = {
		'@context': 'https://schema.org',
		'@type': 'SoftwareApplication',
		name: 'Cydonia',
		description,
		applicationCategory: 'ProductivityApplication',
		operatingSystem: 'macOS',
		url: site,
		downloadUrl: repo,
		license: 'https://opensource.org/licenses/MIT',
		offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
		keywords: [
			'ACP client',
			'Agent Client Protocol',
			'coding agent desktop app',
			'notes app for developers',
			'local-first notes',
			'MCP servers'
		]
	};
	const jsonLdHtml = `<script type="application/ld+json">${JSON.stringify(jsonLd)}<\/script>`;
</script>

<svelte:head>
	<title>Cydonia — a workspace for your notes and your agents</title>
	<meta name="description" content={description} />
	<meta property="og:title" content="Cydonia — a workspace for your notes and your agents" />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="website" />
	<meta property="og:image" content="{site}/og.png" />
	<meta name="twitter:card" content="summary_large_image" />
	{@html jsonLdHtml}
</svelte:head>

<section class="hero">
	<div class="say">
		<h1>Your notes and your agents, in the same window.</h1>
		<p class="lede">
			A directory becomes a workspace. Your writing stays in it as files, and any agent that speaks
			the <a href={acp}>Agent Client Protocol</a> works in the same project.
		</p>

		<div class="cta">
			<a class="button primary" href="#download">
				<Brand icon={siApple} size={16} />
				Download
			</a>
			<a class="button" href={repo}>
				<Brand icon={siGithub} size={16} />
				Source
			</a>
		</div>

		<p class="facts">macOS on Apple silicon · no account, no sync</p>
	</div>

	<Frame ratio="4 / 3" />
</section>

{#if showcase}
	<section class="scenes">
		<nav class="outline">
			<ol>
				{#each scenes as scene, i (scene.id)}
					<li class:current={active === i}>
						<a href="#{scene.id}">{scene.title}</a>
					</li>
				{/each}
			</ol>
		</nav>

		<div class="reel">
			{#each scenes as scene, i (scene.id)}
				<article id={scene.id} bind:this={nodes[i]}>
					<h2>{scene.title}</h2>
					<!-- eslint-disable-next-line svelte/no-at-html-tags -->
					<p>{@html scene.body}</p>
					<Frame ratio="16 / 10" />
				</article>
			{/each}
		</div>
	</section>
{/if}

<section class="own">
	<h2>It stays yours</h2>
	<p>Markdown, SVG, one SQLite file and a TOML config — all on your disk.</p>

	<dl class="paths">
		{#each paths as [path, what] (path)}
			<div>
				<dt>{path}</dt>
				<dd>{what}</dd>
			</div>
		{/each}
	</dl>
</section>

<section class="get" id="download">
	<h2>Download cydonia</h2>
	<p>No packaged builds yet. Install from source with <a href="https://rustup.rs">Rust</a>.</p>

	<div class="install code-block">
		<code>{install}</code>
		<button class="copy" type="button" aria-label="Copy">
			<!-- eslint-disable-next-line svelte/no-at-html-tags -->
			{@html Copy}{@html Check}
		</button>
	</div>
</section>

<footer>
	<nav class="left">
		<a href="https://github.com/crabtalk">crabtalk</a>
	</nav>
	<nav class="right">
		<a href={repo} aria-label="Cydonia on GitHub"><Brand icon={siGithub} size={16} /></a>
		<a href={author} target="_blank" rel="noreferrer" aria-label="The author on X">
			<Brand icon={siX} size={15} />
		</a>
	</nav>
</footer>

<style>
	section {
		max-width: 1080px;
		margin: 0 auto;
		padding: 0 28px;
	}

	.hero {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1.08fr);
		align-items: center;
		gap: 56px;
		padding-top: 72px;
		padding-bottom: 88px;
	}

	h1 {
		margin: 0;
		font-size: clamp(36px, 4.6vw, 52px);
		font-weight: 600;
		letter-spacing: -0.03em;
	}

	.lede {
		max-width: 44ch;
		margin: 22px 0 0;
		color: var(--muted);
		font-size: 17px;
	}

	.lede a,
	.reel a,
	.get a {
		text-decoration: underline;
		text-underline-offset: 3px;
		text-decoration-color: var(--line-strong);
	}

	.cta {
		display: flex;
		gap: 12px;
		margin-top: 30px;
	}

	.button {
		display: inline-flex;
		align-items: center;
		gap: 9px;
		height: 46px;
		padding: 0 22px;
		border: 1px solid var(--line-strong);
		border-radius: 11px;
		font-size: 15px;
		font-weight: 500;
	}

	.button:hover {
		background: var(--panel);
		text-decoration: none;
	}

	.button.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--accent-ink);
	}

	.button.primary:hover {
		background: var(--accent-hover);
		border-color: var(--accent-hover);
	}

	.facts {
		margin: 20px 0 0;
		color: var(--faint);
		font-size: 14px;
	}

	.scenes {
		display: grid;
		grid-template-columns: 190px minmax(0, 1fr);
		gap: 56px;
		padding-bottom: 96px;
	}

	/* Stays put while the reel moves past it, so the rule on its left reads as
	   the spine of this whole stretch of the page. */
	.outline {
		position: sticky;
		top: 96px;
		align-self: start;
	}

	.outline ol {
		display: grid;
		gap: 14px;
		margin: 0;
		padding: 4px 0 4px 18px;
		border-left: 1px solid var(--line);
		list-style: none;
	}

	.outline li {
		position: relative;
		font-size: 14.5px;
	}

	.outline a {
		color: var(--faint);
	}

	.outline a:hover {
		color: var(--muted);
		text-decoration: none;
	}

	.outline .current a {
		color: var(--text);
	}

	/* The marker sits on the rule itself, so the active scene is named on the
	   line rather than beside it. */
	.outline .current::before {
		content: '';
		position: absolute;
		left: -19px;
		top: 7px;
		width: 1px;
		height: 14px;
		background: var(--text);
	}

	.reel {
		display: grid;
		gap: 88px;
	}

	.reel h2 {
		margin: 0 0 10px;
		font-size: clamp(22px, 2.6vw, 28px);
		font-weight: 600;
		letter-spacing: -0.03em;
	}

	.reel p {
		max-width: 52ch;
		margin: 0 0 24px;
		color: var(--muted);
	}

	.own {
		padding-bottom: 88px;
	}

	.own h2 {
		margin: 0 0 10px;
		font-size: clamp(24px, 3vw, 32px);
		font-weight: 600;
		letter-spacing: -0.03em;
	}

	.own p {
		max-width: 52ch;
		margin: 0;
		color: var(--muted);
	}

	.paths {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
		gap: 12px;
		margin: 32px 0 0;
	}

	.paths div {
		padding: 16px 18px;
		border: 1px solid var(--line);
		border-radius: 12px;
	}

	.paths dt {
		font-family: var(--mono);
		font-size: 13px;
	}

	.paths dd {
		margin: 6px 0 0;
		color: var(--muted);
		font-size: 14px;
	}

	.get {
		padding-bottom: 96px;
		text-align: center;
	}

	.get h2 {
		margin: 0;
		font-size: clamp(26px, 3.4vw, 36px);
		font-weight: 600;
		letter-spacing: -0.03em;
	}

	.get p {
		margin: 14px 0 0;
		color: var(--muted);
	}

	.install {
		display: flex;
		align-items: center;
		justify-content: center;
		max-width: 540px;
		margin: 28px auto 0;
		padding: 16px 20px;
		border: 1px solid var(--line);
		border-radius: 12px;
		background: var(--panel);
		overflow-x: auto;
	}

	.install code {
		background: none;
		padding: 0;
		font-size: 14px;
		white-space: nowrap;
	}

	.install :global(.copy) {
		top: 50%;
		right: 10px;
		transform: translateY(-50%);
	}

	footer {
		display: flex;
		align-items: center;
		gap: 20px;
		max-width: 1080px;
		margin: 0 auto;
		padding: 0 28px 56px;
		font-size: 14px;
	}

	footer nav {
		display: flex;
		align-items: center;
		gap: 20px;
	}

	footer .left a {
		color: var(--muted);
	}

	footer .right {
		margin-left: auto;
		color: var(--muted);
	}

	footer a:hover {
		color: var(--text);
	}

	@media (max-width: 940px) {
		.hero,
		.scenes {
			grid-template-columns: minmax(0, 1fr);
			gap: 36px;
		}

		.hero {
			padding-top: 48px;
			padding-bottom: 56px;
		}

		/* No room for a rail beside the reel, and a sticky bar over it would
		   cover the thing it indexes. */
		.outline {
			display: none;
		}

		.reel {
			gap: 64px;
		}
	}

	@media (max-width: 720px) {
		section,
		footer {
			padding-left: 18px;
			padding-right: 18px;
		}

		.cta {
			flex-wrap: wrap;
		}
	}
</style>
