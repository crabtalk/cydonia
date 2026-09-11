<script>
	import { Download, Tag } from 'lucide-static';
	import { anchor, day, groups, media, releases } from '$lib/changelog.js';
	import Media from '$lib/Media.svelte';
	import { dmgFor, releaseFor, site } from '$lib/meta.js';

	const description =
		'Every release of Cydonia — what is new, what changed and what is fixed in each version.';
</script>

<svelte:head>
	<title>Cydonia — changelog</title>
	<meta name="description" content={description} />
	<meta property="og:title" content="Cydonia — changelog" />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="website" />
	<meta property="og:image" content="{site}/og.png" />
	<meta name="twitter:card" content="summary_large_image" />
</svelte:head>

<section class="log">
	<h1>Changelog</h1>

	<ol>
		{#each releases as release (release.version)}
			<li id={anchor(release.version)}>
				<div class="head">
					<a class="num" href="#{anchor(release.version)}">{release.version}</a>
					<span class="day">{day(release.date)}</span>
				</div>

				<div class="body">
					{#if release.summary}
						<p class="summary">{release.summary}</p>
					{/if}

					{#if media(release)}
						<Media media={media(release)} />
					{/if}

					<div class="links">
						<a href={releaseFor(release.version)} target="_blank" rel="noreferrer">
							<!-- eslint-disable-next-line svelte/no-at-html-tags -->
							{@html Tag}
							v{release.version}
						</a>
						<a href={dmgFor(release.version)}>
							<!-- eslint-disable-next-line svelte/no-at-html-tags -->
							{@html Download}
							cydonia-{release.version}-arm64.dmg
						</a>
					</div>

					{#each groups(release) as group (group.title)}
						<h2>{group.title}</h2>
						<ul>
							{#each group.items as item (item)}
								<li>{item}</li>
							{/each}
						</ul>
					{/each}
				</div>
			</li>
		{/each}
	</ol>
</section>

<style>
	.log {
		max-width: 1080px;
		margin: 0 auto;
		padding: 48px var(--gutter) 96px;
	}

	h1 {
		margin: 0;
		font-size: clamp(28px, 3.4vw, 38px);
		font-weight: 600;
		letter-spacing: -0.03em;
	}

	ol {
		margin: 40px 0 0;
		padding: 0;
		list-style: none;
	}

	ol > li {
		padding: 36px 0;
		border-top: 1px solid var(--line);
		scroll-margin-top: 24px;
	}

	ol > li:first-child {
		padding-top: 0;
		border-top: 0;
	}

	/* The version is a rail beside its notes once there is room for one; below
	   that it is a line above them. */
	@media (min-width: 820px) {
		ol > li {
			display: grid;
			grid-template-columns: 140px minmax(0, 1fr);
			gap: 32px;
		}
	}

	.head {
		margin-bottom: 16px;
	}

	.num {
		font-family: var(--mono);
		font-size: 15px;
		font-weight: 500;
	}

	.day {
		display: block;
		margin-top: 4px;
		color: var(--faint);
		font-size: 13.5px;
	}

	.links {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 20px;
		margin-top: 18px;
	}

	/* Both name the thing they fetch — a tag and a file — so both are set in the
	   face the rest of the page uses for names of that kind. */
	.links a {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		color: var(--muted);
		font-family: var(--mono);
		font-size: 13px;
	}

	.links :global(svg) {
		width: 14px;
		height: 14px;
	}

	.body {
		min-width: 0;
	}

	.summary {
		max-width: 68ch;
		margin: 0;
		font-size: 18px;
		line-height: 1.5;
	}

	h2 {
		margin: 28px 0 0;
		color: var(--muted);
		font-size: 12px;
		font-weight: 500;
		letter-spacing: 0.14em;
		text-transform: uppercase;
	}

	ul {
		max-width: 72ch;
		margin: 12px 0 0;
		padding-left: 20px;
	}

	ul li {
		margin-top: 10px;
		color: var(--muted);
	}

	@media (min-width: 820px) {
		.head {
			margin-bottom: 0;
		}
	}
</style>
