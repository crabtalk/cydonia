import { translateAlong } from './kit/iso-kit.js';

// Starts the motion; the returned step advances it by dt seconds.
export const animate = ({ root, P, cells }) => {
    const bit = root.querySelector('[data-part="bit"]');
    const dust = root.querySelector('[data-part="dust"]');
    const cores = [...root.querySelectorAll('[data-cell]')];
    let filled = 0, clock = 0;
    const ease = (x) => x < 0 ? 0 : x > 1 ? 1 : x * x * (3 - 2 * x);
    return (dt) => {
      clock += dt;
      const t = clock % 5.2;
      const down = ease(t / 1.1) - ease((t - 3.1) / 1.1);
      const drilling = t > 1.1 && t < 3.1;
      const jitter = drilling ? Math.sin(clock * 90) * 0.25 : 0;
      bit.setAttribute("transform", translateAlong(P, [jitter, 0, -down * 9.6]));
      dust.setAttribute("opacity", drilling ? (0.5 + 0.5 * Math.sin(clock * 14)).toFixed(2) : "0");
      const cycle = Math.floor(clock / 5.2);
      if (t >= 4.2 && filled <= cycle) {
        if (filled % cells === 0) cores.forEach((c) => delete c.dataset.on);
        cores[filled % cells].dataset.on = "";
        filled++;
      }
    };
  };
