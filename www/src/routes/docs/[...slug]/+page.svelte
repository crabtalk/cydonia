<script>
	import { href } from '$lib/docs/nav.js';
	import { site } from '$lib/meta.js';

	let { data } = $props();

	const canonical = $derived(`${site}${href(data.slug)}`);
	const heading = $derived(
		data.slug === 'index' ? 'Cydonia documentation' : `${data.title} — Cydonia docs`
	);

	// Whichever heading owns the top of the viewport is the one the column
	// marks — the same rule the front page's reel follows.
	let active = $state('');

	$effect(() => {
		if (!data.toc.length) return;
		const observer = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) if (entry.isIntersecting) active = entry.target.id;
			},
			{ rootMargin: '0px 0px -75% 0px' }
		);
		for (const { id } of data.toc) {
			const node = document.getElementById(id);
			if (node) observer.observe(node);
		}
		return () => observer.disconnect();
	});
</script>

<svelte:head>
	<title>{heading}</title>
	{#if data.description}
		<meta name="description" content={data.description} />
		<meta property="og:description" content={data.description} />
	{/if}
	<meta property="og:title" content={heading} />
	<link rel="canonical" href={canonical} />
</svelte:head>

<article>
	<h1>{data.title}</h1>
	{#if data.description}
		<p class="lede">{data.description}</p>
	{/if}

	<!-- Markdown from `docs/`, rendered at build time. -->
	<div class="prose">{@html data.html}</div>

	<nav class="steps">
		{#if data.prev}
			<a class="step" href={href(data.prev.slug)}>
				<span>Previous</span>
				{data.prev.title}
			</a>
		{:else}
			<span></span>
		{/if}
		{#if data.next}
			<a class="step next" href={href(data.next.slug)}>
				<span>Next</span>
				{data.next.title}
			</a>
		{/if}
	</nav>
</article>

{#if data.toc.length}
	<nav class="toc" aria-label="On this page">
		<p class="caption">On this page</p>
		{#each data.toc as entry (entry.id)}
			<a href="#{entry.id}" class:deep={entry.depth === 3} class:here={active === entry.id}>
				{entry.text}
			</a>
		{/each}
	</nav>
{/if}

<style>
	article {
		min-width: 0;
		padding: 28px 0 72px;
	}

	h1 {
		margin: 0;
		font-size: 30px;
		font-weight: 600;
	}

	.lede {
		margin: 10px 0 0;
		color: var(--muted);
		font-size: 17px;
	}

	.prose {
		margin-top: 32px;
	}

	/* Everything below is global: the body is html this component was handed,
	   not markup Svelte compiled in place. */
	/* A heading jumped to from the contents column would otherwise land under
	   the bar. */
	.prose :global(h2),
	.prose :global(h3) {
		scroll-margin-top: calc(var(--header) + 20px);
	}

	.prose :global(h2) {
		margin: 40px 0 12px;
		font-size: 20px;
		font-weight: 600;
	}

	.prose :global(h3) {
		margin: 28px 0 10px;
		font-size: 16px;
		font-weight: 600;
	}

	/* The autolink wrap makes each heading a link to itself. It should read as
	   a heading until the pointer is on it. */
	.prose :global(h2 a),
	.prose :global(h3 a) {
		color: inherit;
		font-weight: inherit;
		text-decoration: none;
	}

	.prose :global(p),
	.prose :global(li) {
		color: var(--muted);
	}

	.prose :global(strong) {
		color: var(--text);
		font-weight: 550;
	}

	.prose :global(ul),
	.prose :global(ol) {
		padding-left: 20px;
	}

	.prose :global(li) {
		margin: 6px 0;
	}

	.prose :global(a) {
		text-decoration: underline;
		text-underline-offset: 3px;
		text-decoration-color: var(--line-strong);
	}

	.prose :global(pre) {
		margin: 18px 0;
	}

	.prose :global(table) {
		width: 100%;
		border-collapse: collapse;
		font-size: 14px;
	}

	.prose :global(th) {
		color: var(--muted);
		font-weight: 500;
		background: var(--panel);
	}

	.prose :global(th),
	.prose :global(td) {
		padding: 9px 12px;
		border-bottom: 1px solid var(--line);
		text-align: left;
		vertical-align: top;
	}

	.prose :global(blockquote) {
		margin: 18px 0;
		padding: 2px 0 2px 16px;
		border-left: 2px solid var(--line-strong);
		color: var(--muted);
	}

	.steps {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		margin-top: 56px;
		padding-top: 24px;
		border-top: 1px solid var(--line);
	}

	.step {
		display: grid;
		gap: 2px;
		font-size: 14px;
		font-weight: 500;
	}

	.step.next {
		text-align: right;
	}

	.step span {
		color: var(--faint);
		font-size: 12px;
		font-weight: 400;
	}

	@media (hover: hover) {
		.step:hover {
			text-decoration: none;
			color: var(--text);
		}
	}

	.toc {
		position: sticky;
		top: calc(var(--header) + 28px);
		align-self: start;
		max-height: calc(100vh - var(--header) - 40px);
		overflow-y: auto;
		padding-bottom: 24px;
	}

	.caption {
		margin: 0 0 8px;
		padding-left: 13px;
		color: var(--faint);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}

	.toc a {
		display: block;
		padding: 4px 0 4px 12px;
		border-left: 1px solid var(--line);
		color: var(--muted);
		font-size: 13px;
	}

	.toc a.deep {
		padding-left: 24px;
	}

	.toc a.here {
		border-left-color: var(--text);
		color: var(--text);
	}

	@media (hover: hover) {
		.toc a:hover {
			color: var(--text);
			text-decoration: none;
		}
	}

	/* Below this the page needs the width more than the column does. */
	@media (max-width: 1100px) {
		.toc {
			display: none;
		}
	}
</style>
