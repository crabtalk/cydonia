<script>
	// The map at the foot of the home page, seen as through a window: the frame stays put
	// while the scene behind it (static/terrain-plate.svg, centred on the Face, larger
	// than the window by its overscan) drifts against it with scroll and pointer. The
	// light follows the pointer over the ground and rests on the Face.
	import { base } from '$app/paths';
	import { follow } from '$lib/follow.js';

	// The window and the scene, in the SVG's units: the scene is the window plus overscan.
	const WINDOW = [776, 228];
	const SCENE = [900, 264];
	// Largest drift as a share of the window, kept inside the overscan.
	const SCROLL_DRIFT = 0.04;
	const POINTER_DRIFT = 0.025;

	let frame = $state();
	let scene = $state();

	$effect(() => {
		const still = matchMedia('(prefers-reduced-motion: reduce)');
		let pointer = [50, 50];
		let scroll = 0;
		let queued = 0;

		function draw() {
			const box = frame.getBoundingClientRect();
			const moving = !still.matches;
			const dx = moving ? (-(pointer[0] - 50) / 50) * POINTER_DRIFT : 0;
			const dy = moving ? (-(pointer[1] - 50) / 50) * POINTER_DRIFT + scroll * SCROLL_DRIFT : 0;
			scene.style.transform = `translate(calc(-50% + ${(dx * box.width).toFixed(2)}px), calc(-50% + ${(dy * box.height).toFixed(2)}px))`;
			// The pointer's place on the scene, for the light.
			const sx = (SCENE[0] / WINDOW[0] - 1) / 2;
			const sy = (SCENE[1] / WINDOW[1] - 1) / 2;
			const x = ((pointer[0] / 100 + sx - dx) / (1 + 2 * sx)) * 100;
			const y = ((pointer[1] / 100 + sy - dy) / (1 + 2 * sy)) * 100;
			scene.style.setProperty('--x', `${x}%`);
			scene.style.setProperty('--y', `${y}%`);
		}

		const light = follow((point) => {
			pointer = point;
			draw();
		});
		light.aim([50, 50]);

		// -1 when the window is a screen above the middle of the viewport, 1 a screen below.
		const onScroll = () => {
			if (queued) return;
			queued = requestAnimationFrame(() => {
				queued = 0;
				const box = frame.getBoundingClientRect();
				scroll = Math.max(-1, Math.min(1, (box.top + box.height / 2 - innerHeight / 2) / innerHeight));
				draw();
			});
		};
		const move = (event) => {
			if (event.pointerType === 'touch') return;
			const box = frame.getBoundingClientRect();
			light.aim([((event.clientX - box.left) / box.width) * 100, ((event.clientY - box.top) / box.height) * 100]);
		};
		const leave = () => light.aim([50, 50]);

		onScroll();
		addEventListener('scroll', onScroll, { passive: true });
		addEventListener('resize', onScroll);
		frame.addEventListener('pointermove', move);
		frame.addEventListener('pointerleave', leave);
		return () => {
			light.stop();
			cancelAnimationFrame(queued);
			removeEventListener('scroll', onScroll);
			removeEventListener('resize', onScroll);
			frame.removeEventListener('pointermove', move);
			frame.removeEventListener('pointerleave', leave);
		};
	});
</script>

<figure class="plate">
	<div
		class="window"
		style:aspect-ratio="{WINDOW[0]} / {WINDOW[1]}"
		bind:this={frame}
		data-own-light
		role="img"
		aria-label="Contour map of Cydonia Mensae on Mars, with the mesa known as the Face at 40.75°N 9.46°W."
	>
		<div
			class="scene"
			style:--map="url('{base}/terrain-plate.svg')"
			style:width="{(SCENE[0] / WINDOW[0]) * 100}%"
			style:height="{(SCENE[1] / WINDOW[1]) * 100}%"
			bind:this={scene}
		>
			<div class="lines"></div>
			<div class="light"></div>
			<span class="face"></span>
			<span class="where">40.75°N 9.46°W</span>
		</div>
	</div>
	<figcaption>
		<span>Cydonia Mensae, Mars</span>
		<span>Contours every 40 m · MOLA, NASA</span>
	</figcaption>
</figure>

<style>
	.plate {
		margin: 0;
	}

	.window {
		position: relative;
		border: 1px solid var(--line);
		background: var(--bg);
		overflow: hidden;
	}

	/* The frame's own depth: shade cast inward onto the scene behind it. */
	.window::after {
		content: '';
		position: absolute;
		inset: 0;
		box-shadow: inset 0 0 36px 10px rgb(0 0 0 / 0.85);
		pointer-events: none;
	}

	.scene {
		position: absolute;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		will-change: transform;
	}

	.lines,
	.light {
		position: absolute;
		inset: 0;
		-webkit-mask: var(--map) center / 100% 100% no-repeat;
		mask: var(--map) center / 100% 100% no-repeat;
	}

	.lines {
		background: rgb(255 255 255 / 0.11);
	}

	.light {
		--light: radial-gradient(circle 180px at var(--x, 50%) var(--y, 50%), #000 0%, rgb(0 0 0 / 0.5) 45%, transparent 100%);
		background: rgb(255 255 255 / 0.42);
		-webkit-mask:
			var(--map) center / 100% 100% no-repeat,
			var(--light);
		-webkit-mask-composite: source-in;
		mask:
			var(--map) center / 100% 100% no-repeat,
			var(--light);
		mask-composite: intersect;
	}

	.face {
		position: absolute;
		left: 50%;
		top: 50%;
		width: 26px;
		height: 26px;
		border: 1px solid var(--muted);
		border-radius: 50%;
		transform: translate(-50%, -50%);
	}

	.where {
		position: absolute;
		left: calc(50% + 22px);
		top: calc(50% - 24px);
		font: 11px/1 var(--mono);
		color: var(--muted);
		white-space: nowrap;
	}

	figcaption {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		padding-top: 12px;
		font: 11px/1.4 var(--mono);
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--faint);
	}
</style>
