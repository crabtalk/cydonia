<script>
	import { base } from '$app/paths';
	import Media from '$lib/Media.svelte';

	// With `media`, the recording plays until the reader asks for the app
	// itself; without it, the wasm is fetched once the box scrolls into view.
	// `show` is what the app opens on — see the demo crate's `shown` — and a
	// change to it reloads the frame.
	let { show = undefined, media = undefined } = $props();

	let src = $derived(`${base}/demo/index.html${show ? `?show=${show}` : ''}`);

	// The narrowest width the app is laid out at. A narrower box shows it
	// scaled down rather than reflowed.
	const WIDTH = 1080;

	let width = $state(WIDTH);
	let scale = $derived(Math.min(1, width / WIDTH));

	let box = $state();
	let running = $state(false);

	$effect(() => {
		if (media || !box || running) return;
		const observer = new IntersectionObserver(
			([entry]) => {
				if (entry.isIntersecting) running = true;
			},
			{ threshold: 0.25 }
		);
		observer.observe(box);
		return () => observer.disconnect();
	});
</script>

<div class="demo" class:recording={media && !running} bind:this={box} bind:clientWidth={width}>
	{#if running}
		<iframe
			title="Cydonia, running"
			{src}
			style:width="{100 / scale}%"
			style:height="{100 / scale}%"
			style:transform="scale({scale})"
		></iframe>
	{:else if media}
		<Media {media} />
		<button class="run" type="button" onclick={() => (running = true)}>Run it here</button>
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

	/* The recording sets the box's size at its own ratio. */
	.demo.recording {
		aspect-ratio: auto;
	}

	.demo :global(.media) {
		margin: 0;
		border: 0;
	}

	iframe {
		display: block;
		border: 0;
		transform-origin: 0 0;
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

	@media (hover: hover) {
		.run {
			opacity: 0;
		}

		.run:hover {
			background: var(--accent-hover);
		}

		.demo:hover .run,
		.run:focus-visible {
			opacity: 1;
		}
	}
</style>
