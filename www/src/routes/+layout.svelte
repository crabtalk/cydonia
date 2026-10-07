<script>
	import '../app.css';
	import { siDiscord, siGithub, siX } from 'simple-icons';
	import { base } from '$app/paths';
	import Brand from '$lib/Brand.svelte';
	import Logo from '$lib/Logo.svelte';
	import Crab from '$lib/Crab.svelte';
	import Glyphs from '$lib/Glyphs.svelte';
	import Signature from '$lib/Signature.svelte';
	import { crabtalk, discord, repo } from '$lib/meta.js';
	import { page } from '$app/state';

	const author = 'https://x.com/tianyi_gc';

	let { children } = $props();

	// The docs are as wide as the bar, 1400px; the home page 1200px, the other
	// pages 1080px. The foot follows the page's width.
	const docs = $derived(page.url.pathname.startsWith('/docs'));
	const home = $derived(page.url.pathname === `${base}/` || page.url.pathname === base);
	const shell = $derived(docs ? '1400px' : home ? '1200px' : '1080px');

	// One delegated handler for the whole site: every `.code-block` gets a working
	// copy button without an `onclick` of its own.
	async function copy(event) {
		const button = event.target.closest?.('.copy');
		if (!button) return;
		const code = button.parentElement?.querySelector('code');
		if (!code) return;
		try {
			await navigator.clipboard.writeText(code.textContent ?? '');
			button.classList.add('copied');
			setTimeout(() => button.classList.remove('copied'), 1400);
		} catch (error) {
			console.warn('copy failed', error);
		}
	}

	$effect(() => {
		document.addEventListener('click', copy);
		return () => document.removeEventListener('click', copy);
	});
</script>

<header>
	<div class="bar" class:wide={docs}>
		<a class="wordmark" href="{base}/">
			<Logo size={18} />
			Cydonia
		</a>

		<nav>
			<a class="docs" href="{base}/docs/">Docs</a>
			<span class="icons">
				<a href={repo} aria-label="Cydonia on GitHub"><Brand icon={siGithub} size={15} /></a>
				<a href={discord} target="_blank" rel="noreferrer" aria-label="Cydonia on Discord">
					<Brand icon={siDiscord} size={16} />
				</a>
			</span>
			<a class="control button" href="{base}/#download">
				Download
			</a>
		</nav>
	</div>
</header>

{#if home}
	<Signature />
	<Glyphs />
{/if}

<div class="page">
	{@render children()}

	{#if !docs}
		<footer style:--shell={shell}>
			<nav class="left">
				<a class="by" href={crabtalk}>
					<Crab size={14} />
					crabtalk
				</a>
			</nav>
			<nav class="right">
				<a href={discord} target="_blank" rel="noreferrer" aria-label="Cydonia on Discord">
					<Brand icon={siDiscord} size={16} />
				</a>
				<a href={repo} aria-label="Cydonia on GitHub"><Brand icon={siGithub} size={16} /></a>
				<a href={author} target="_blank" rel="noreferrer" aria-label="The author on X">
					<Brand icon={siX} size={15} />
				</a>
			</nav>
		</footer>
	{/if}
</div>

<style>
	/* Above the glyphs, which are fixed behind it. */
	.page {
		position: relative;
		z-index: 1;
	}

	footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		max-width: var(--shell);
		margin: 0 auto;
		padding: 0 var(--gutter) 28px;
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

	.by {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}

	footer .right {
		gap: 4px;
		color: var(--muted);
	}

	footer .right a {
		display: grid;
		place-items: center;
		width: 42px;
		height: 42px;
	}

	@media (hover: hover) {
		footer a:hover {
			color: var(--text);
		}
	}

	/* A band across the window, with the page's own column inside it: pinned,
	   the row alone would leave the scrolling page showing either side of it. */
	header {
		position: sticky;
		top: 0;
		z-index: 20;
		height: var(--header);
		border-bottom: 1px solid var(--line);
		background: color-mix(in srgb, var(--bg) 85%, transparent);
		backdrop-filter: blur(16px);
		-webkit-backdrop-filter: blur(16px);
	}

	.bar {
		display: flex;
		align-items: center;
		height: 100%;
		max-width: calc(1400px + 48px);
		margin: 0 auto;
		padding: 0 24px;
	}

	.bar.wide {
		max-width: none;
	}

	@media (max-width: 600px) {
		.bar {
			padding: 0 16px;
		}
	}

	.wordmark {
		display: inline-flex;
		align-items: center;
		gap: 9px;
		flex: none;
		font-size: 17px;
		font-weight: 600;
		letter-spacing: -0.02em;
	}

	/* The rule underlines the mark along with the word, which reads as a strike
	   through the logo rather than a link. */
	@media (hover: hover) {
		.wordmark:hover {
			text-decoration: none;
		}
	}

	.bar nav {
		display: flex;
		align-items: center;
		gap: 20px;
		margin-left: auto;
		font-size: 14.5px;
	}

	.docs {
		color: var(--muted);
		font-size: 14px;
	}

	@media (hover: hover) {
		.docs:hover {
			color: var(--text);
			text-decoration: none;
		}
	}

	/* Square ghost controls the height of the button beside them, ruled off
	   from it. */
	.icons {
		display: flex;
		align-items: center;
		gap: 2px;
		margin-left: -8px;
		padding-right: 12px;
		border-right: 1px solid var(--line);
		color: var(--muted);
	}

	.icons a {
		display: grid;
		place-items: center;
		width: var(--control-height);
		height: var(--control-height);
		border-radius: var(--radius);
	}

	@media (hover: hover) {
		.icons a:hover {
			background: var(--panel-high);
			color: var(--text);
		}
	}

	.button {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		border-radius: var(--radius);
		background: var(--accent);
		color: var(--accent-ink);
		font-weight: 500;
	}

	@media (hover: hover) {
		.button:hover {
			background: var(--accent-hover);
			text-decoration: none;
		}
	}
</style>
