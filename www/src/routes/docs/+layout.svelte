<script>
	import { page } from '$app/state';
	import { siDiscord, siGithub, siX } from 'simple-icons';
	import Brand from '$lib/Brand.svelte';
	import Crab from '$lib/Crab.svelte';
	import { href } from '$lib/docs/nav.js';
	import { crabtalk, discord, repo } from '$lib/meta.js';

	const author = 'https://x.com/tianyi_gc';

	let { children, data } = $props();

	const sections = $derived(data.sections);

	const current = $derived(page.url.pathname.replace(/\/$/, ''));
	const here = (slug) => current === href(slug).replace(/\/$/, '');

	/** The list is a column beside the page and a fold above it. Open to begin
	    with, for the column, and folded on a screen too narrow for one.

	    Not a `details`: a browser wraps its contents in a `::details-content`
	    box, which leaves the list and the foot children of that box rather
	    than of the column — and the column is what puts the foot at its
	    bottom. */
	let open = $state(true);

	$effect(() => {
		const narrow = window.matchMedia('(max-width: 860px)');
		const fold = () => (open = !narrow.matches);
		fold();
		narrow.addEventListener('change', fold);
		return () => narrow.removeEventListener('change', fold);
	});
</script>

<div class="docs">
	<!-- One list, two shapes: a column beside the page on a desk, a disclosure
	     above it on a phone. -->
	<nav class="nav" class:open>
		<button class="fold" aria-expanded={open} onclick={() => (open = !open)}>Documentation</button>

		<div class="list">
			{#each sections as section, index (section.title + index)}
				<div class="section">
					{#if section.title}
						<p class="caption">{section.title}</p>
					{/if}
					{#each section.pages as entry (entry.slug)}
						<a href={href(entry.slug)} class:here={here(entry.slug)}>{entry.title}</a>
					{/each}
				</div>
			{/each}
		</div>

		<!-- The site's foot, which the docs do not carry across the page: the
		     column is where a reader is already looking for a way out. -->
		<div class="foot">
			<a class="by" href={crabtalk}>
				<Crab size={13} />
				crabtalk
			</a>
			<span class="marks">
				<a href={discord} target="_blank" rel="noreferrer" aria-label="Cydonia on Discord">
					<Brand icon={siDiscord} size={15} />
				</a>
				<a href={repo} aria-label="Cydonia on GitHub"><Brand icon={siGithub} size={15} /></a>
				<a href={author} target="_blank" rel="noreferrer" aria-label="The author on X">
					<Brand icon={siX} size={14} />
				</a>
			</span>
		</div>
	</nav>

	{@render children()}
</div>

<style>
	.docs {
		display: grid;
		grid-template-columns: 204px minmax(0, 1fr) 200px;
		gap: 40px;
		max-width: 1400px;
		margin: 0 auto;
		padding: 0 14px;
	}

	/* A column the height of the window under the bar: the rule down its right
	   only reads as one while it runs the whole way, and the foot only sits at
	   the bottom while there is a bottom to sit at. */
	.nav {
		display: flex;
		flex-direction: column;
		position: sticky;
		top: var(--header);
		align-self: start;
		height: calc(100vh - var(--header));
		padding: 20px 12px 14px 0;
		border-right: 1px solid var(--line);
	}

	.list {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
	}

	/* Beside the page there is nothing to fold, so the control is gone and the
	   list is always out. */
	.fold {
		display: none;
	}

	/* The rows are pills; without a gap two of them share an edge and the one
	   the page is on reads as a block rather than a mark. */
	.section {
		display: grid;
		gap: 2px;
	}

	.section + .section {
		margin-top: 16px;
	}

	.caption {
		margin: 0 0 3px;
		padding: 0 6px;
		color: var(--faint);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}

	.nav a {
		display: block;
		padding: 3px 6px;
		border-radius: var(--radius);
		color: var(--muted);
		font-size: 13.5px;
		line-height: 1.5;
	}

	@media (hover: hover) {
		.nav a:hover {
			background: var(--panel-high);
			color: var(--text);
			text-decoration: none;
		}
	}

	.nav a.here {
		background: var(--panel-high);
		color: var(--text);
		font-weight: 500;
	}

	/* The page keeps the width the column gives up. */
	.foot {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-top: 16px;
		padding: 12px 6px 0;
		border-top: 1px solid var(--line);
		color: var(--faint);
		font-size: 13px;
	}

	.marks {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-left: auto;
	}

	.foot a {
		display: inline-flex;
		align-items: center;
		padding: 0;
		color: var(--faint);
	}

	.by {
		gap: 7px;
	}

	@media (hover: hover) {
		.foot a:hover {
			color: var(--text);
			text-decoration: none;
		}
	}

	@media (max-width: 1100px) {
		.docs {
			grid-template-columns: 232px minmax(0, 1fr);
		}
	}

	@media (max-width: 860px) {
		.docs {
			grid-template-columns: minmax(0, 1fr);
			gap: 24px;
		}

		.nav {
			position: static;
			margin-top: 20px;
			height: auto;
			padding: 0 0 4px;
			border-right: 0;
			border-bottom: 1px solid var(--line);
		}

		.list {
			overflow-y: visible;
		}

		.foot {
			margin-top: 16px;
		}

		.fold {
			display: block;
			width: 100%;
			padding: 8px 0 12px;
			border: 0;
			background: none;
			color: var(--text);
			font: inherit;
			font-size: 14px;
			font-weight: 500;
			text-align: left;
			cursor: pointer;
		}

		.nav:not(.open) .list,
		.nav:not(.open) .foot {
			display: none;
		}
	}
</style>
