<script>
	import '../app.css';
	import { siDiscord } from 'simple-icons';
	import { base } from '$app/paths';
	import Brand from '$lib/Brand.svelte';
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
	<a class="wordmark" href="{base}/">Cydonia</a>

	<nav>
		<a class="community" href={discord} target="_blank" rel="noreferrer">
			<Brand icon={siDiscord} size={16} />
			Community
		</a>
		<a class="button" href="{base}/#download">Download</a>
	</nav>
</header>

{@render children()}

<style>
	header {
		display: flex;
		align-items: center;
		max-width: 1080px;
		margin: 0 auto;
		padding: 0 28px;
		height: 68px;
	}

	.wordmark {
		font-size: 17px;
		font-weight: 600;
		letter-spacing: -0.02em;
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

	.community:hover {
		color: var(--text);
		text-decoration: none;
	}

	.button {
		display: inline-flex;
		align-items: center;
		height: 36px;
		padding: 0 16px;
		border-radius: 9px;
		background: var(--accent);
		color: var(--accent-ink);
		font-weight: 500;
	}

	.button:hover {
		background: var(--accent-hover);
		text-decoration: none;
	}

	@media (max-width: 720px) {
		header {
			padding: 0 18px;
		}

		nav {
			gap: 16px;
		}
	}
</style>
