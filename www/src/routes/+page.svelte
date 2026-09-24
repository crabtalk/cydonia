<script>
	import ShareImage from '$lib/ShareImage.svelte';
	import { siGithub } from 'simple-icons';
	import DownloadPanel from '$lib/DownloadPanel.svelte';
	import Brand from '$lib/Brand.svelte';
	import Files from '$lib/Files.svelte';
	import Frame from '$lib/Frame.svelte';
	import Media from '$lib/Media.svelte';
	import { base } from '$app/paths';
	import { anchor, day, latest, media, releases } from '$lib/changelog.js';
	import { repo, site, tagline as description } from '$lib/meta.js';

	let { data } = $props();

	const featured = releases.find((release) => media(release));
	const featureMedia = featured ? media(featured) : null;
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
		operatingSystem: 'macOS, Linux, Windows',
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
	<title>Cydonia — where agents keep their work</title>
	<meta name="description" content={description} />
	<meta property="og:title" content="Cydonia — where agents keep their work" />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="website" />
	{@html jsonLdHtml}
</svelte:head>

<ShareImage />

<section class="hero">
	<div class="say">
		<h1>Where agents keep their work.</h1>
		<div class="hero-actions">
			<div class="cta">
				<a class="control button primary" href="#download">Download</a>
				<a class="control button" href={repo}>
					<Brand icon={siGithub} size={14} />
					Source
				</a>
			</div>
			<p class="facts">pure rust · no account, no sync</p>
		</div>
	</div>

	{#if featured && featureMedia}
		<figure class="feature">
			<Media media={featureMedia} />
			<figcaption>
				{#if featured.summary}
					<p>{featured.summary}</p>
				{/if}
				<a href="{base}/changelog/#{anchor(featured.version)}">
					What’s new in {featured.version} <span aria-hidden="true">→</span>
				</a>
			</figcaption>
		</figure>
	{/if}
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

	<Files roots={data.roots} />
</section>

<section class="get" id="download">
	<h2>Try Cydonia</h2>

	<div class="head">
		<a class="num" href="{base}/changelog/#{anchor(latest.version)}">{latest.version}</a>
		<span class="tag">Latest</span>
		<span class="day">{day(latest.date)}</span>
		<a class="release-link" href="{base}/changelog/#{anchor(latest.version)}">Release notes</a>
	</div>

	<div class="release-content">
		<DownloadPanel version={latest.version} />
	</div>
</section>

<style>
	.release-content {
		margin-top: 24px;
	}

	section {
		max-width: 1080px;
		margin: 0 auto;
		padding: 0 var(--gutter);
	}

	.hero {
		display: grid;
		gap: 40px;
		padding-top: 72px;
		padding-bottom: 88px;
	}

	.say {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 40px;
	}

	.hero-actions {
		flex-shrink: 0;
		padding-bottom: 4px;
	}

	h1 {
		margin: 0;
		max-width: 17ch;
		font-size: clamp(36px, 4.6vw, 52px);
		text-wrap: balance;
		font-weight: 600;
		letter-spacing: -0.03em;
	}

	.feature {
		min-width: 0;
		margin: 0;
	}

	.feature :global(.media) {
		width: 100%;
		height: auto;
		margin: 0;
		border: 0;
		border-radius: var(--radius-lg);
		background: transparent;
	}

	.feature figcaption {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 16px 40px;
		margin-top: 16px;
		font-size: 13px;
	}

	.feature figcaption a {
		flex-shrink: 0;
	}

	.feature p {
		max-width: 72ch;
		margin: 0;
		color: var(--muted);
	}

	.reel a {
		text-decoration: underline;
		text-underline-offset: 3px;
		text-decoration-color: var(--line-strong);
	}

	.cta {
		display: flex;
		gap: 12px;
	}

	.button {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius);
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

	/* Browsers without scroll-driven animations show the scenes as they are. */
	@media (prefers-reduced-motion: no-preference) {
		@supports (animation-timeline: view()) {
			.reel article {
				animation: reveal linear both;
				animation-timeline: view();
				animation-range: entry 0% entry 40%;
			}
		}
	}

	@keyframes reveal {
		from {
			opacity: 0;
		}
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

	.get {
		scroll-margin-top: calc(var(--header) + 24px);
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

	/* Release metadata stays separate from platform choices. */
	.head {
		display: flex;
		flex-wrap: wrap;
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
		border: 1px solid var(--line);
		color: var(--muted);
		font-family: var(--mono);
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.day {
		color: var(--faint);
		font-size: 13.5px;
	}

	.release-link {
		font-size: 13px;
		color: var(--muted);
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
		.say,
		.feature figcaption {
			flex-direction: column;
			align-items: flex-start;
			gap: 24px;
		}

		.feature figcaption {
			gap: 8px;
		}

		.cta {
			flex-wrap: wrap;
		}
	}
</style>
