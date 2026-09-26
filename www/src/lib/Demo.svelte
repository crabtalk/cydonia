<script>
	import { base } from '$app/paths';
	import Media from '$lib/Media.svelte';

	// The recording plays until the reader asks for the app itself; only then is
	// the wasm fetched.
	let { media } = $props();

	let running = $state(false);
</script>

<div class="demo">
	{#if running}
		<iframe title="Cydonia, running" src="{base}/demo/index.html"></iframe>
	{:else}
		{#if media}
			<Media {media} />
		{/if}
		<button class="run" class:alone={!media} type="button" onclick={() => (running = true)}>
			Run it here
		</button>
	{/if}
</div>

<style>
	.demo {
		position: relative;
		aspect-ratio: 16 / 10;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--panel);
		overflow: hidden;
	}

	.demo :global(.media) {
		display: block;
		width: 100%;
		height: 100%;
		margin: 0;
		border: 0;
		border-radius: 0;
		object-fit: cover;
		background: transparent;
	}

	iframe {
		display: block;
		width: 100%;
		height: 100%;
		border: 0;
	}

	/* Top right, clear of the video's own controls along the bottom. Shown on
	   hover where there is hover; always where there is not. */
	.run {
		position: absolute;
		top: 12px;
		right: 12px;
		padding: 7px 12px;
		border: 0;
		border-radius: var(--radius);
		background: var(--accent);
		color: var(--accent-ink);
		font: inherit;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		transition: opacity 0.12s;
	}

	.run:hover {
		background: var(--accent-hover);
	}

	@media (hover: hover) {
		.run:not(.alone) {
			opacity: 0;
		}

		.demo:hover .run,
		.run:focus-visible {
			opacity: 1;
		}
	}

	.run.alone {
		top: 50%;
		right: 50%;
		transform: translate(50%, -50%);
	}
</style>
