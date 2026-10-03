<script>
	import ShareImage from '$lib/ShareImage.svelte';
	import { siGithub } from 'simple-icons';
	import DownloadPanel from '$lib/DownloadPanel.svelte';
	import Demo from '$lib/Demo.svelte';
	import Brand from '$lib/Brand.svelte';
	import Crab from '$lib/Crab.svelte';
	import Files from '$lib/Files.svelte';
	import Frame from '$lib/Frame.svelte';
	import { base } from '$app/paths';
	import { releasePath, day, latest, media, releases } from '$lib/changelog.js';
	import { repo, site, tagline as description } from '$lib/meta.js';

	let { data } = $props();

	const featured = releases.find((release) => media(release)) ?? latest;
	const featureMedia = media(featured);
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
	<meta property="og:url" content={`${site}/`} />
	<link rel="canonical" href={`${site}/`} />
	{@html jsonLdHtml}
</svelte:head>

<ShareImage />

<!-- Fixed behind the page at the top: only a pull past the top shows it. -->
<div class="signature" aria-hidden="true">
	<p>Built with <span class="heart">♥</span> by the crabtalk team</p>
	<span class="crab"><span class="step"><Crab size={12} /></span></span>
</div>

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

	<figure class="feature">
		<Demo media={featureMedia} />
		{#if featured}
			<figcaption>
				{#if featured.summary}
					<p>{featured.summary}</p>
				{/if}
				<a href="{base}{releasePath(featured.version)}">
					What’s new in {featured.version} <span aria-hidden="true">→</span>
				</a>
			</figcaption>
		{/if}
	</figure>
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
	.signature {
		position: fixed;
		inset: 0 0 auto;
		z-index: -1;
		display: flex;
		align-items: center;
		justify-content: center;
		height: var(--header);
		color: var(--faint);
		font-size: 13px;
	}

	/* A band of light across the words, then a rest. */
	.signature p {
		margin: 0;
		background: linear-gradient(
			100deg,
			var(--faint) 40%,
			var(--text) 50%,
			var(--faint) 60%
		);
		background-size: 300% 100%;
		background-clip: text;
		-webkit-background-clip: text;
		-webkit-text-fill-color: transparent;
		animation: shine 4.5s ease-in-out infinite;
	}

	@keyframes shine {
		0% {
			background-position: 100% 0;
		}
		33%,
		100% {
			background-position: 0 0;
		}
	}

	.heart {
		display: inline-block;
		color: #e5484d;
		-webkit-text-fill-color: #e5484d;
		animation: beat 1.2s ease-in-out infinite;
	}

	/* Lub-dub, then rest. */
	@keyframes beat {
		0%,
		40%,
		100% {
			transform: scale(1);
		}
		10% {
			transform: scale(1.3);
		}
		20% {
			transform: scale(1);
		}
		30% {
			transform: scale(1.15);
		}
	}

	/* Along the foot of the band and back, rocking as it goes, turning to
	   face the way it walks at each edge — and blushing the heart's red as it
	   turns. */
	.crab {
		position: absolute;
		bottom: 6px;
		left: 12px;
		animation:
			scuttle 32s ease-in-out infinite,
			blush 32s linear infinite;
	}

	.step {
		display: block;
		animation: step 0.18s linear infinite alternate;
	}

	@keyframes scuttle {
		0%,
		100% {
			left: 12px;
			transform: scaleX(1);
		}
		49.9% {
			left: calc(100% - 24px);
			transform: scaleX(1);
		}
		50% {
			left: calc(100% - 24px);
			transform: scaleX(-1);
		}
		99.9% {
			left: 12px;
			transform: scaleX(-1);
		}
	}

	@keyframes blush {
		0%,
		50%,
		100% {
			color: #e5484d;
		}
		6%,
		44%,
		56%,
		94% {
			color: var(--faint);
		}
	}

	@keyframes step {
		from {
			transform: translateY(0) rotate(-8deg);
		}
		to {
			transform: translateY(-1px) rotate(8deg);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.signature p,
		.heart,
		.crab,
		.step {
			animation: none;
		}
	}

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
