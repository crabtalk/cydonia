<script>
	import '../app.css';
	import { siApple, siDiscord } from 'simple-icons';
	import { base } from '$app/paths';
	import Brand from '$lib/Brand.svelte';
	import Logo from '$lib/Logo.svelte';
	import { discord } from '$lib/meta.js';

	let { children } = $props();

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
	<a class="wordmark" href="{base}/">
		<Logo size={18} />
		Cydonia
	</a>

	<nav>
		<a
			class="community"
			href={discord}
			target="_blank"
			rel="noreferrer"
			aria-label="Cydonia community on Discord"
		>
			<Brand icon={siDiscord} size={16} />
			<span>Community</span>
		</a>
		<a class="button" href="{base}/#download">
			<Brand icon={siApple} size={16} />
			Download
		</a>
	</nav>
</header>

{@render children()}

<style>
	header {
		display: flex;
		align-items: center;
		max-width: 1080px;
		margin: 0 auto;
		padding: 0 var(--gutter);
		height: 68px;
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

	nav {
		display: flex;
		align-items: center;
		gap: 20px;
		margin-left: auto;
		font-size: 14.5px;
	}

	.community {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		color: var(--muted);
	}

	@media (hover: hover) {
		.community:hover {
			color: var(--text);
			text-decoration: none;
		}
	}

	.button {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 36px;
		padding: 0 16px;
		border-radius: 9px;
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

	/* On a phone the three of these together are wider than the bar. The mark
	   alone still says Discord, and the link keeps its name for screen readers. */
	@media (max-width: 560px) {
		nav {
			gap: 14px;
		}

		.community span {
			display: none;
		}
	}
</style>
