<script>
	import { base } from '$app/paths';
	import ShareImage from '$lib/ShareImage.svelte';
	import ReleaseNotes from '$lib/ReleaseNotes.svelte';
	import { day, releasePath } from '$lib/changelog.js';
	import { site, releaseFor } from '$lib/meta.js';
	let { data } = $props();
	const release = $derived(data.release);
	const title = $derived(`Cydonia v${release.version}`);
	const description = $derived(release.summary || `Release notes for ${title}.`);
	const canonical = $derived(`${site}${releasePath(release.version)}`);
</script>

<svelte:head>
	<title>{title} — release notes</title>
	<meta name="description" content={description} />
	<meta property="og:title" content={title} />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="article" />
	<meta property="og:url" content={canonical} />
	<link rel="canonical" href={canonical} />
</svelte:head>

<ShareImage card={`releases/${release.version}`} />

<article>
	<a class="back" href={`${base}/changelog/`}>← All releases</a>
	<h1>{title}</h1>
	<div class="meta">
		<time datetime={release.date}>{day(release.date)}</time>
		<a href={releaseFor(release.version)}>View on GitHub ↗</a>
	</div>
	<ReleaseNotes {release} />
</article>

<style>
	article { max-width: 1080px; margin: 0 auto; padding: 48px var(--gutter) 96px; }
	.back, .meta { font-size: 14px; color: var(--muted); }
	h1 { margin: 28px 0 12px; font-size: clamp(28px, 3.4vw, 38px); letter-spacing: -0.03em; }
	.meta { display: flex; flex-wrap: wrap; gap: 24px; margin-bottom: 36px; }
</style>
