// The backdrop's drawing: the relief's contours on one canvas, coloured by height, a
// tide of light climbing through the levels, and particles streaming along the
// contours on a second canvas that keeps their trails.

const PARTICLES = 2000;
const TIDE_SECONDS = 14;
const TIDE_WIDTH = 0.07;
const DRIFT_SECONDS = 90;
const TRAIL_FADE = 0.06;
const SPEED = 9;
const STIR_RADIUS = 140;

/** Gradients from the low ground to the high, each colour a stop. */
export const palettes = {
	mars: ['#5b1a6e', '#b3305f', '#f0603a', '#ffae45', '#fff1c2'],
	aurora: ['#2e3a8c', '#3a7bd5', '#5bc0be', '#6fffe9', '#e0fff6'],
	bloom: ['#480ca8', '#7209b7', '#b5179e', '#f72585', '#ffc6e5'],
	ember: ['#1a659e', '#5fa8d3', '#efefd0', '#f7c59f', '#ff6b35'],
	reef: ['#6a4c93', '#1982c4', '#8ac926', '#ffca3a', '#ff595e']
};

const rgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));

/** 256 colours along a palette, there and back, so a drifting offset wraps without a seam. */
function ramp(stops) {
	const colours = stops.map(rgb);
	return Array.from({ length: 256 }, (_, i) => {
		const u = 1 - Math.abs((i / 255) * 2 - 1);
		const at = u * (colours.length - 1);
		const k = Math.min(colours.length - 2, Math.floor(at));
		const f = at - k;
		return colours[k].map((v, j) => Math.round(v + (colours[k + 1][j] - v) * f));
	});
}

export class Relief {
	/** `ground` draws the contours, `trails` the particles; `relief` is static/terrain-relief.json. */
	constructor(ground, trails, relief) {
		this.ground = ground.getContext('2d');
		this.trails = trails.getContext('2d');
		this.canvases = [ground, trails];
		this.relief = relief;
		const lo = relief.levels[0].m;
		const hi = relief.levels.at(-1).m;
		this.levels = relief.levels.map(({ m, d }) => ({ t: (m - lo) / (hi - lo || 1), index: m % relief.index === 0, path: new Path2D(d) }));
		this.lo = lo;
		this.hi = hi;
		this.time = 0;
		this.stir = null;
		this.palette('mars');
	}

	palette(name) {
		this.colours = ramp(palettes[name] ?? palettes.mars);
		this.clearTrails();
	}

	/** Fits the map to cover a viewport of `width` by `height` CSS px. */
	resize(width, height, ratio) {
		for (const canvas of this.canvases) {
			canvas.width = Math.round(width * ratio);
			canvas.height = Math.round(height * ratio);
		}
		const { width: w, height: h } = this.relief;
		this.scale = Math.max(width / w, height / h);
		this.offset = [(width - w * this.scale) / 2, (height - h * this.scale) / 2];
		this.ratio = ratio;
		this.view = [-this.offset[0] / this.scale, -this.offset[1] / this.scale, width / this.scale, height / this.scale];
		this.particles = Array.from({ length: PARTICLES }, () => this.spawn({}));
		this.clearTrails();
	}

	clearTrails() {
		if (!this.ratio) return;
		const { trails } = this;
		trails.setTransform(1, 0, 0, 1, 0, 0);
		trails.clearRect(0, 0, trails.canvas.width, trails.canvas.height);
	}

	/** Points the stir at a viewport position, or takes it away with null. */
	point(at) {
		this.stir = at && [(at[0] - this.offset[0]) / this.scale, (at[1] - this.offset[1]) / this.scale];
	}

	/** Height in metres at map position x, y, bilinear between the field's samples. */
	height(x, y) {
		const { cols, rows, cell, heights } = this.relief.field;
		const fx = Math.min(cols - 1.001, Math.max(0, x / cell[0]));
		const fy = Math.min(rows - 1.001, Math.max(0, y / cell[1]));
		const c = Math.floor(fx);
		const r = Math.floor(fy);
		const u = fx - c;
		const v = fy - r;
		const at = (rr, cc) => heights[rr * cols + cc];
		return (at(r, c) * (1 - u) + at(r, c + 1) * u) * (1 - v) + (at(r + 1, c) * (1 - u) + at(r + 1, c + 1) * u) * v;
	}

	colour(t, alpha) {
		const drift = this.time / DRIFT_SECONDS;
		const [r, g, b] = this.colours[Math.floor((((t * 0.5 + drift) % 1) + 1) % 1 * 255)];
		return `rgb(${r} ${g} ${b} / ${alpha})`;
	}

	/** How lit a level at height `t` is by the tide, 0 to 1. */
	tide(t) {
		const at = (this.time / TIDE_SECONDS) % 1.3 - 0.15;
		return Math.exp(-((t - at) ** 2) / (2 * TIDE_WIDTH * TIDE_WIDTH));
	}

	spawn(particle) {
		const [x, y, w, h] = this.view;
		particle.x = x + Math.random() * w;
		particle.y = y + Math.random() * h;
		particle.age = 0;
		particle.life = 2 + Math.random() * 6;
		particle.weight = 0.4 + Math.random() * 1.4;
		particle.speed = 0.5 + Math.random();
		return particle;
	}

	/** Advances `dt` seconds and draws. */
	frame(dt) {
		this.time += dt;
		this.drawGround();
		this.drawTrails(dt);
	}

	drawGround() {
		const { ground, scale, offset, ratio } = this;
		ground.setTransform(1, 0, 0, 1, 0, 0);
		ground.clearRect(0, 0, ground.canvas.width, ground.canvas.height);
		ground.setTransform(scale * ratio, 0, 0, scale * ratio, offset[0] * ratio, offset[1] * ratio);
		ground.lineJoin = 'round';
		ground.globalCompositeOperation = 'lighter';
		const px = 1 / scale;
		for (const { t, index, path } of this.levels) {
			const lit = this.tide(t);
			ground.lineWidth = (index ? 1.3 : 0.7) * px * (1 + lit);
			ground.strokeStyle = this.colour(t, (index ? 0.34 : 0.16) + 0.66 * lit);
			if (lit > 0.3) {
				ground.shadowColor = this.colour(t, lit);
				ground.shadowBlur = 14 * lit * ratio;
			}
			ground.stroke(path);
			ground.shadowBlur = 0;
			if (lit > 0.2) {
				ground.setLineDash([2 * px, 22 * px]);
				ground.lineDashOffset = -this.time * 40 * px;
				ground.lineWidth = 2.4 * px;
				ground.strokeStyle = `rgb(255 255 255 / ${0.7 * lit})`;
				ground.stroke(path);
				ground.setLineDash([]);
			}
		}
	}

	drawTrails(dt) {
		const { trails, scale, offset, ratio } = this;
		trails.setTransform(1, 0, 0, 1, 0, 0);
		trails.globalCompositeOperation = 'destination-out';
		trails.fillStyle = `rgb(0 0 0 / ${1 - Math.pow(1 - TRAIL_FADE, dt * 60)})`;
		trails.fillRect(0, 0, trails.canvas.width, trails.canvas.height);
		trails.setTransform(scale * ratio, 0, 0, scale * ratio, offset[0] * ratio, offset[1] * ratio);
		trails.globalCompositeOperation = 'lighter';
		trails.lineCap = 'round';
		const [vx, vy, vw, vh] = this.view;
		const e = 1.5;
		for (const p of this.particles) {
			// Along the contour: the gradient turned a quarter, so a particle keeps its height.
			const gx = this.height(p.x + e, p.y) - this.height(p.x - e, p.y);
			const gy = this.height(p.x, p.y + e) - this.height(p.x, p.y - e);
			const slope = Math.hypot(gx, gy);
			let dx = -gy / (slope || 1);
			let dy = gx / (slope || 1);
			// Flat ground has no contour to follow; it drifts in a slow swirl instead.
			const flat = Math.exp(-slope / 4);
			const swirl = Math.sin(p.x * 0.02 + this.time * 0.1) * 3 + Math.cos(p.y * 0.025 - this.time * 0.07) * 3;
			dx = dx * (1 - flat) + Math.cos(swirl) * flat;
			dy = dy * (1 - flat) + Math.sin(swirl) * flat;
			if (this.stir) {
				const sx = p.x - this.stir[0];
				const sy = p.y - this.stir[1];
				const d = Math.hypot(sx, sy);
				const pull = Math.max(0, 1 - (d * scale) / STIR_RADIUS);
				dx += (-sy / (d || 1)) * pull * 3;
				dy += (sx / (d || 1)) * pull * 3;
			}
			const step = SPEED * p.speed * dt;
			const nx = p.x + dx * step;
			const ny = p.y + dy * step;
			p.age += dt;
			const t = (this.height(p.x, p.y) - this.lo) / (this.hi - this.lo || 1);
			const fade = Math.sin(Math.min(1, p.age / p.life) * Math.PI);
			trails.lineWidth = p.weight / scale;
			trails.strokeStyle = this.colour(t, fade * (0.35 + 0.5 * this.tide(t)));
			trails.beginPath();
			trails.moveTo(p.x, p.y);
			trails.lineTo(nx, ny);
			trails.stroke();
			p.x = nx;
			p.y = ny;
			if (p.age > p.life || p.x < vx || p.y < vy || p.x > vx + vw || p.y > vy + vh) this.spawn(p);
		}
	}
}
