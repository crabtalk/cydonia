<script>
	import ShareImage from '$lib/ShareImage.svelte';
	import DownloadPanel from '$lib/DownloadPanel.svelte';
	import Media from '$lib/Media.svelte';
	import Brand from '$lib/Brand.svelte';
	import Files from '$lib/Files.svelte';
	import Figure from '$lib/Figure.svelte';
	import { base } from '$app/paths';
	import { releasePath, day, latest, media, releases } from '$lib/changelog.js';
	import { agents, repo, site, tagline as description } from '$lib/meta.js';
	import { hero } from '$lib/media.js';

	let { data } = $props();

	// The hero plays one pinned recording; its caption follows the latest release.
	const pinned = releases.find((release) => release.version === hero);
	const featureMedia = media(pinned ?? releases.find((release) => media(release)) ?? latest);
	const acp = 'https://agentclientprotocol.com';

	const scenes = [
		{
			id: 'open',
			figure: 'cabinet',
			title: 'Open a directory',
			body: 'Any folder becomes a project. What you write lands in <code>.cydonia/</code> inside it, gitignored.'
		},
		{
			id: 'write',
			figure: 'typewriter',
			title: 'Write it down',
			body: 'Articles with covers and highlighted code. Boards and tables when a thought wants columns.'
		},
		{
			id: 'hand',
			figure: 'rover',
			title: 'Hand it over',
			body: 'Any agent that speaks <a href="' +
				acp +
				'">ACP</a> works in the project. Its edits land in the window you were writing in.'
		}
	];

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
			'agent control panel',
			...agents.map((agent) => agent.name),
			'ACP client',
			'Agent Client Protocol',
			'agent orchestrator',
			'coding agent desktop app',
			'local-first workspace',
			'MCP servers',
			'markdown articles',
			'kanban boards for agents'
		]
	};
	const jsonLdHtml = `<script type="application/ld+json">${JSON.stringify(jsonLd)}<\/script>`;
</script>

<svelte:head>
	<title>Cydonia — control panel for coding agents, with articles and boards</title>
	<meta name="description" content={description} />
	<meta property="og:title" content="Cydonia — control panel for coding agents" />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="website" />
	<meta property="og:url" content={`${site}/`} />
	<link rel="canonical" href={`${site}/`} />
	{@html jsonLdHtml}
</svelte:head>

<ShareImage />

<section class="hero">
	<div class="say">
		<h1>Where agents keep their work.</h1>
		<div class="hero-actions">
			<p class="agents">
				{#each agents as agent (agent.name)}
					<span role="img" aria-label={agent.name} title={agent.name}>
						<Brand icon={agent.icon} size={16} />
					</span>
				{/each}
				<span>and any <a href={acp}>ACP</a> agent</span>
			</p>
			<div class="cta">
				<a class="control button primary" href="#download">Download</a>
			</div>
		</div>
	</div>

	<figure class="feature">
		{#if featureMedia}
			<Media media={featureMedia} />
		{/if}
		{#if latest}
			<figcaption>
				{#if latest.summary}
					<p>{latest.summary}</p>
				{/if}
				<div class="aside">
					<a href="{base}{releasePath(latest.version)}">
						What’s new in {latest.version} <span aria-hidden="true">→</span>
					</a>
				</div>
			</figcaption>
		{/if}
	</figure>
</section>

<section class="scenes">
	<div class="sheet">
		<div class="title-block" aria-hidden="true">
			<span>Cydonia · how it works</span>
		</div>
		<div class="cells">
			{#each scenes as scene, i (scene.id)}
				<article id={scene.id}>
					<span class="fig" aria-hidden="true">Fig {i + 1}</span>
					<Figure name={scene.figure} />
					<h2>{scene.title}</h2>
					<!-- eslint-disable-next-line svelte/no-at-html-tags -->
					<p>{@html scene.body}</p>
				</article>
			{/each}
		</div>
	</div>
</section>

<section class="own">
	<h2>The work outlives the session</h2>
	<p>Markdown, SVG, one SQLite file and a TOML config — all on your disk, all yours.</p>

	<Files roots={data.roots} />
</section>

<section class="get" id="download">
	<h2>Try Cydonia</h2>

	<div class="head">
		<a class="num" href="{base}{releasePath(latest.version)}">{latest.version}</a>
		<span class="tag">Latest</span>
		<span class="day">{day(latest.date)}</span>
		<a class="release-link" href="{base}{releasePath(latest.version)}">Release notes</a>
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
		max-width: calc(1200px + 2 * var(--gutter));
		margin: 0 auto;
		padding: 0 var(--gutter);
	}

	/* The space between two sections is the lower one's top padding. No section
	   pads its own bottom, apart from the last, which ends the page. */
	section + section {
		padding-top: clamp(80px, 11vw, 144px);
	}

	.hero {
		display: grid;
		gap: 40px;
		padding-top: 72px;
	}

	.say {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 40px;
	}

	.hero-actions {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
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
		margin: 0;
	}

	.feature figcaption {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 16px 40px;
		margin-top: 16px;
		font-size: 13px;
	}

	.aside {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		flex-shrink: 0;
		gap: 2px;
	}

	.feature p {
		max-width: 72ch;
		margin: 0;
		color: var(--muted);
	}

	.scenes :global(a) {
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

	.agents {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 6px 14px;
		max-width: 46ch;
		margin: 0 0 20px;
		color: var(--muted);
		font-size: 13px;
		line-height: 1.5;
	}

	.agents span {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}

	/* The logos on one line, the sentence on its own below them. */
	.agents span:last-child {
		flex-basis: 100%;
		justify-content: inherit;
		color: var(--faint);
	}

	.agents a {
		color: inherit;
		text-decoration: underline;
		text-underline-offset: 3px;
	}

	/* A drawing sheet: one ruled box, a title block along the top, one cell
	   per figure. Figures fill their solids with the sheet's own colour. */
	.sheet {
		--figure-paper: var(--panel);
		border: 1px solid var(--line);
		background: var(--panel);
	}

	.title-block {
		padding: 10px 16px;
		border-bottom: 1px solid var(--line);
		background: var(--panel);
		font: 11px/1.4 var(--mono);
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--faint);
	}

	.cells {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
	}

	.cells article {
		position: relative;
		padding: 40px 28px 28px;
	}

	.cells article + article {
		border-left: 1px solid var(--line);
	}

	.fig {
		position: absolute;
		top: 12px;
		left: 16px;
		font: 11px/1.4 var(--mono);
		color: var(--faint);
	}

	/* Browsers without scroll-driven animations show the scenes as they are. */
	@media (prefers-reduced-motion: no-preference) {
		@supports (animation-timeline: view()) {
			.scenes article {
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

	.scenes h2 {
		margin: 20px 0 8px;
		font-size: 20px;
		font-weight: 600;
		letter-spacing: -0.03em;
	}

	.scenes p {
		margin: 0;
		color: var(--muted);
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
		.hero {
			grid-template-columns: minmax(0, 1fr);
			gap: 36px;
		}

		.cells {
			grid-template-columns: minmax(0, 1fr);
		}

		.cells article + article {
			border-left: 0;
			border-top: 1px solid var(--line);
		}

		.hero {
			padding-top: 48px;
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

		.aside,
		.hero-actions {
			align-items: flex-start;
		}

		.agents {
			justify-content: flex-start;
		}

		.cta {
			flex-wrap: wrap;
		}
	}
</style>
