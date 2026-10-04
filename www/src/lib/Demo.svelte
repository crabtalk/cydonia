<script>
	import { base } from '$app/paths';

	// The wasm is fetched once the box scrolls into view. `show` is what the
	// app opens on — see the demo crate's `shown` — and a change to it reloads
	// the frame.
	let { show } = $props();

	let src = $derived(`${base}/demo/index.html${show ? `?show=${show}` : ''}`);

	// The narrowest width the app is laid out at. A narrower box shows it
	// scaled down rather than reflowed.
	const WIDTH = 1080;

	let width = $state(WIDTH);
	let scale = $derived(Math.min(1, width / WIDTH));

	let box = $state();
	let running = $state(false);

	$effect(() => {
		if (!box || running) return;
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

<div class="demo" bind:this={box} bind:clientWidth={width}>
	{#if running}
		<iframe
			title="Cydonia, running"
			{src}
			style:width="{100 / scale}%"
			style:height="{100 / scale}%"
			style:transform="scale({scale})"
		></iframe>
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

	iframe {
		display: block;
		border: 0;
		transform-origin: 0 0;
	}
</style>
