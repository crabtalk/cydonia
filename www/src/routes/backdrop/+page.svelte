<script>
	// The home page's terrain alone, full screen and in motion, for filming behind the
	// app. Keys: p the next palette, space pause, r clear the trails, s save a PNG.
	// ?palette= picks the palette on load. The pointer stirs the particles and hides
	// after IDLE_MS still.
	import { base } from '$app/paths';
	import { replaceState } from '$app/navigation';
	import { Relief, palettes } from './relief.js';

	const IDLE_MS = 1500;

	let ground = $state();
	let trails = $state();
	let idle = $state(false);

	$effect(() => {
		let relief = null;
		let frame = 0;
		let last = 0;
		let paused = false;
		let timer = 0;
		let name = new URLSearchParams(location.search).get('palette');
		if (!(name in palettes)) name = 'mars';
		const still = matchMedia('(prefers-reduced-motion: reduce)');

		const fit = () => relief?.resize(innerWidth, innerHeight, Math.min(devicePixelRatio, 2));
		const tick = (now) => {
			const dt = last ? Math.min((now - last) / 1000, 1 / 20) : 1 / 60;
			last = now;
			if (!paused) relief.frame(dt);
			frame = still.matches ? 0 : requestAnimationFrame(tick);
		};
		const move = (event) => {
			idle = false;
			clearTimeout(timer);
			timer = setTimeout(() => (idle = true), IDLE_MS);
			if (event.pointerType !== 'touch') relief?.point([event.clientX, event.clientY]);
		};
		const leave = () => relief?.point(null);
		const key = (event) => {
			if (!relief || event.metaKey || event.ctrlKey) return;
			if (event.key === 'p') {
				const names = Object.keys(palettes);
				name = names[(names.indexOf(name) + 1) % names.length];
				relief.palette(name);
				replaceState(`?palette=${name}`, {});
			} else if (event.key === ' ') {
				event.preventDefault();
				paused = !paused;
			} else if (event.key === 'r') relief.clearTrails();
			else if (event.key === 's') save();
		};
		const save = () => {
			const out = document.createElement('canvas');
			out.width = ground.width;
			out.height = ground.height;
			const context = out.getContext('2d');
			context.fillStyle = getComputedStyle(document.body).backgroundColor;
			context.fillRect(0, 0, out.width, out.height);
			context.drawImage(ground, 0, 0);
			context.globalCompositeOperation = 'lighter';
			context.drawImage(trails, 0, 0);
			const link = document.createElement('a');
			link.download = `cydonia-backdrop-${name}-${Date.now()}.png`;
			link.href = out.toDataURL('image/png');
			link.click();
		};

		const loading = new AbortController();
		fetch(`${base}/terrain-relief.json`, { signal: loading.signal })
			.then((response) => response.json())
			.then((data) => {
				relief = new Relief(ground, trails, data);
				relief.palette(name);
				fit();
				frame = requestAnimationFrame(tick);
			})
			.catch((error) => {
				if (error.name !== 'AbortError') console.warn('backdrop: relief failed to load', error);
			});

		addEventListener('resize', fit);
		addEventListener('pointermove', move);
		addEventListener('keydown', key);
		document.documentElement.addEventListener('pointerleave', leave);
		return () => {
			loading.abort();
			cancelAnimationFrame(frame);
			clearTimeout(timer);
			removeEventListener('resize', fit);
			removeEventListener('pointermove', move);
			removeEventListener('keydown', key);
			document.documentElement.removeEventListener('pointerleave', leave);
		};
	});
</script>

<svelte:head>
	<title>Cydonia — backdrop</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<div class="backdrop" class:idle aria-hidden="true">
	<canvas bind:this={ground}></canvas>
	<canvas class="trails" bind:this={trails}></canvas>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		overflow: hidden;
		background: var(--bg);
	}

	.idle {
		cursor: none;
	}

	canvas {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
	}

	.trails {
		mix-blend-mode: screen;
	}
</style>
