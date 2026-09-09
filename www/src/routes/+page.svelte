<script>
	import { siApple, siGithub } from 'simple-icons';
	import { Check, Copy, Download } from 'lucide-static';
	import Brand from '$lib/Brand.svelte';
	import Frame from '$lib/Frame.svelte';
	import Media from '$lib/Media.svelte';
	import { base } from '$app/paths';
	import { anchor, day, latest, media } from '$lib/changelog.js';
	import {
		cdn,
		dmg,
		dmgFor,
		install,
		repo,
		site,
		tagline as description
	} from '$lib/meta.js';

	const video = `${cdn}/videos/cydonia/v0.1.0.mp4`;
	const poster = `${cdn}/pics/cydonia/v0.1.0.jpg`;
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
			'agent orchestrator',
			'coding agent desktop app',
			'local-first workspace',
			'MCP servers'
		]
	};
	const jsonLdHtml = `<script type="application/ld+json">${JSON.stringify(jsonLd)}<\/script>`;
</script>

<svelte:head>
	<title>Cydonia — a workspace for the agents you run</title>
	<meta name="description" content={description} />
	<meta property="og:title" content="Cydonia — a workspace for the agents you run" />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="website" />
	<meta property="og:image" content="{site}/og.png" />
	<meta name="twitter:card" content="summary_large_image" />
	{@html jsonLdHtml}
</svelte:head>

<section class="hero">
	<div class="say">
		<h1>Agents that leave something behind.</h1>
		<div class="cta">
			<a class="button primary" href={dmg}>
				<Brand icon={siApple} size={16} />
				Download
			</a>
			<a class="button" href={repo}>
				<Brand icon={siGithub} size={16} />
				Source
			</a>
		</div>

		<p class="facts">macOS on Apple silicon · pure rust · no account, no sync</p>
	</div>

	<!-- The poster is what loads; the 0.9 MB behind it waits for a click. The
	     ratio is spelled out in CSS so the box does not reflow when it does. -->
	<video
		class="demo"
		src={video}
		{poster}
		controls
		loop
		muted
		playsinline
		preload="none"
		aria-label="Cydonia in use"
	></video>
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
	<h2>The work outlives the session</h2>
	<p>Markdown, SVG, one SQLite file and a TOML config — all on your disk, all yours.</p>

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
	<h2>Try Cydonia</h2>

	<div class="head">
		<a class="num" href="{base}/changelog/#{anchor(latest.version)}">{latest.version}</a>
		<span class="tag">Latest</span>
		<span class="day">{day(latest.date)}</span>
	</div>

	{#if latest.summary}
		<p class="summary">{latest.summary}</p>
	{/if}

	{#if media(latest)}
		<Media media={media(latest)} height={280} />
	{/if}

	<a class="dl" href={dmgFor(latest.version)}>
		<!-- eslint-disable-next-line svelte/no-at-html-tags -->
		{@html Download}
		cydonia-{latest.version}-arm64.dmg
	</a>

	<div class="alt">
		<span class="or">or</span>
		<span class="install code-block">
			<code>{install}</code>
			<button class="copy" type="button" aria-label="Copy">
				<!-- eslint-disable-next-line svelte/no-at-html-tags -->
				{@html Copy}{@html Check}
			</button>
		</span>
	</div>
</section>

<style>
	section {
		max-width: 1080px;
		margin: 0 auto;
		padding: 0 var(--gutter);
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

	.demo {
		width: 100%;
		aspect-ratio: 1280 / 804;
		border: 1px solid var(--line);
		border-radius: 14px;
		background: var(--panel);
		object-fit: cover;
	}

	.reel a {
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

	@media (hover: hover) {
		.button:hover {
			background: var(--panel);
			text-decoration: none;
		}
	}

	.button.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--accent-ink);
	}

	@media (hover: hover) {
		.button.primary:hover {
			background: var(--accent-hover);
			border-color: var(--accent-hover);
		}
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

	@media (hover: hover) {
		.outline a:hover {
			color: var(--muted);
			text-decoration: none;
		}
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

	/* The section between the hero and the download had no top padding at all,
	   so it read as a continuation of the hero rather than its own stretch of
	   page. Scales with the viewport instead of needing a breakpoint. */
	.own {
		padding-top: clamp(80px, 11vw, 144px);
		padding-bottom: clamp(80px, 11vw, 144px);
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
	}

	.get h2 {
		margin: 0;
		font-size: clamp(26px, 3.4vw, 36px);
		font-weight: 600;
		letter-spacing: -0.03em;
	}

	.head {
		margin-top: 34px;
	}

	/* Version, badge and day are one line, with the date at the far end so a
	   column of them lines up as the list grows. */
	.head {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.num {
		font-family: var(--mono);
		font-size: 15px;
		font-weight: 500;
	}

	.tag {
		padding: 2px 8px;
		border-radius: 999px;
		background: var(--panel-high);
		color: var(--muted);
		font-size: 12px;
	}

	.day {
		margin-left: auto;
		color: var(--faint);
		font-size: 13.5px;
	}

	.summary {
		max-width: 62ch;
		margin: 12px 0 0;
		color: var(--muted);
	}

	/* Every release names its file the same quiet way. The page's one filled
	   button is in the hero, where a call to action belongs. */
	.dl {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		margin-top: 14px;
		color: var(--muted);
		font-family: var(--mono);
		font-size: 13px;
	}

	.dl :global(svg) {
		width: 14px;
		height: 14px;
	}

	/* The other way in, under the file rather than beside it. A one-liner needs
	   no tab, heading or rule to introduce it — just the word `or`. */
	.alt {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
		margin-top: 30px;
	}

	.or {
		color: var(--faint);
		font-size: 14px;
	}

	.install {
		display: inline-flex;
		align-items: center;
		max-width: 100%;
		padding: 6px 40px 6px 12px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--panel);
		overflow-x: auto;
	}

	.install code {
		background: none;
		padding: 0;
		font-size: 13px;
		/* The body's 1.6 is what made this a block rather than a line. */
		line-height: 1.5;
		white-space: nowrap;
	}

	/* Sized with the box it sits in: the shared 32px — 40px on touch — was
	   built for a panel and is taller than this line. */
	.install :global(.copy) {
		top: 50%;
		right: 5px;
		width: 26px;
		height: 26px;
		transform: translateY(-50%);
	}

	.install :global(.copy svg) {
		width: 13px;
		height: 13px;
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
		.cta {
			flex-wrap: wrap;
		}
	}
</style>
