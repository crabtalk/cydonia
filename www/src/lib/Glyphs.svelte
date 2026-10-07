<script>
	// Fixed behind the page: monospace glyphs light up on a grid, blink and
	// fade. Drawn on one canvas at a low frame rate; with reduced motion, one
	// still frame.
	const CELL = 18;
	const GLYPHS = '01{}[]<>/\\|#$%&*+=~:;acdeinorstuy';
	const TICK = 90;
	const LIFE = 40;

	let canvas = $state();

	$effect(() => {
		const context = canvas.getContext('2d');
		const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
		let cols = 0;
		let rows = 0;
		let lit = new Map();
		let colour = '';

		function size() {
			const ratio = devicePixelRatio || 1;
			canvas.width = innerWidth * ratio;
			canvas.height = innerHeight * ratio;
			context.setTransform(ratio, 0, 0, ratio, 0, 0);
			context.font = `12px ui-monospace, SFMono-Regular, Menlo, monospace`;
			context.textAlign = 'center';
			context.textBaseline = 'middle';
			cols = Math.ceil(innerWidth / CELL);
			rows = Math.ceil(innerHeight / CELL);
			colour = getComputedStyle(canvas).color;
			lit = new Map([...lit].filter(([cell]) => cell < cols * rows));
		}

		function spawn(count) {
			for (let i = 0; i < count; i++) {
				const cell = Math.floor(Math.random() * cols * rows);
				const glyph = GLYPHS[Math.floor(Math.random() * GLYPHS.length)];
				lit.set(cell, { glyph, life: LIFE * (0.4 + Math.random() * 0.6) });
			}
		}

		function draw() {
			context.clearRect(0, 0, innerWidth, innerHeight);
			context.fillStyle = colour;
			for (const [cell, dot] of lit) {
				// A lit glyph flickers off now and then before it fades.
				if (!still && Math.random() < 0.08) continue;
				context.globalAlpha = 0.35 * Math.min(1, dot.life / (LIFE * 0.5));
				context.fillText(dot.glyph, (cell % cols) * CELL + CELL / 2, Math.floor(cell / cols) * CELL + CELL / 2);
			}
		}

		function tick() {
			if (document.hidden) return;
			for (const [cell, dot] of lit) {
				dot.life -= 1;
				if (dot.life <= 0) lit.delete(cell);
				else if (Math.random() < 0.02) dot.glyph = GLYPHS[Math.floor(Math.random() * GLYPHS.length)];
			}
			spawn(Math.max(1, Math.round((cols * rows) / 900)));
			draw();
		}

		size();
		spawn(Math.round((cols * rows) / 40));
		draw();

		const scheme = matchMedia('(prefers-color-scheme: dark)');
		const restyle = () => {
			size();
			draw();
		};
		addEventListener('resize', restyle);
		scheme.addEventListener('change', restyle);
		const timer = still ? 0 : setInterval(tick, TICK);
		return () => {
			clearInterval(timer);
			removeEventListener('resize', restyle);
			scheme.removeEventListener('change', restyle);
		};
	});
</script>

<canvas bind:this={canvas} aria-hidden="true"></canvas>

<style>
	canvas {
		position: fixed;
		inset: 0;
		width: 100vw;
		height: 100vh;
		color: var(--faint);
		pointer-events: none;
	}
</style>
