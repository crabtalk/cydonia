<script>
	// An isometric figure drawn by scripts/generate-figures.mjs. Its SVG is fetched
	// when the frame nears the viewport, and it moves only while on screen.
	import { base } from '$app/paths';
	import './generated/figures/iso.css';

	let { name, ratio = '3 / 2' } = $props();

	const inputs = import.meta.glob('./generated/figures/*.json', { import: 'default', eager: true });
	const motions = import.meta.glob('./figures/*.js', { import: 'animate', eager: true });

	const input = $derived(inputs[`./generated/figures/${name}.json`]);

	let frameNode = $state();
	let drawing = $state();

	$effect(() => {
		const still = matchMedia('(prefers-reduced-motion: reduce)');
		let step = null;
		let frame = 0;
		let last = 0;
		let visible = false;
		let loading = null;
		let gone = false;

		const tick = (now) => {
			const dt = last ? Math.min((now - last) / 1000, 1 / 30) : 1 / 60;
			last = now;
			step(dt);
			frame = visible && !still.matches ? requestAnimationFrame(tick) : 0;
		};
		const run = () => {
			if (step && visible && !frame && !still.matches) {
				last = 0;
				frame = requestAnimationFrame(tick);
			}
		};
		const load = () =>
			(loading ??= fetch(`${base}/figures/${name}.svg`)
				.then((response) => response.text())
				.then((svg) => {
					if (gone) return;
					drawing.innerHTML = svg;
					step = motions[`./figures/${name}.js`]({ root: drawing, ...input });
					run();
				}));

		const near = new IntersectionObserver(([entry]) => entry.isIntersecting && load(), { rootMargin: '600px 0px' });
		const onScreen = new IntersectionObserver(([entry]) => {
			visible = entry.isIntersecting;
			run();
		});
		near.observe(frameNode);
		onScreen.observe(frameNode);
		return () => {
			gone = true;
			near.disconnect();
			onScreen.disconnect();
			cancelAnimationFrame(frame);
		};
	});
</script>

<div class="figure iso" data-theme="dark" style="aspect-ratio: {ratio}" bind:this={frameNode}>
	<div class="drawing" bind:this={drawing}></div>
</div>

<style>
	.figure {
		--anatomy-paper: var(--figure-paper, var(--bg));
		--anatomy-card: var(--figure-paper, var(--bg));
		position: relative;
		width: 100%;
	}

	.drawing {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
	}

	.drawing :global(svg) {
		width: auto;
		height: 100%;
		max-width: 100%;
	}

	.figure :global(.cy-core) {
		fill: var(--anatomy-paper);
		stroke: none;
		opacity: 0;
	}

	.figure :global(.cy-core[data-on]) {
		fill: var(--anatomy-lit);
		opacity: 1;
	}
</style>
