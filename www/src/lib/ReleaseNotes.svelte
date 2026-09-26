<script>
	import { groups, media } from './changelog.js';
	import Media from './Media.svelte';
	let { release } = $props();
</script>

<div class="body">
	{#if release.summary}
		<p class="summary">{release.summary}</p>
	{/if}

	{#if media(release)}
		<Media media={media(release)} />
	{/if}

	{#each groups(release) as group (group.title)}
		<h2>{group.title}</h2>
		<ul>
			{#each group.items as item (item)}
				<li>{item}</li>
			{/each}
		</ul>
	{/each}
</div>

<style>
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

</style>
