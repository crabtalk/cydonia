import { translateAlong } from './kit/iso-kit.js';

// Starts the motion; the returned step advances it by dt seconds.
export const animate = ({ root, P, cols }) => {
    const cardEl = root.querySelector('[data-part="card"]');
    const pickerEl = root.querySelector('[data-part="picker"]');
    const ease = (x) => (x < 0 ? 0 : x > 1 ? 1 : x * x * (3 - 2 * x));
    const OUT = 19;
    let clock = 0;
    return (dt) => {
      clock += dt;
      const t = clock % 10.4;
      const leg = (start, from) => {
        const out = ease((t - start) / 0.8) - ease((t - start - 2) / 0.8);
        const travel = ease((t - start - 0.8) / 1.2);
        return { x: cols[from] + (cols[from + 1] - cols[from]) * travel, y: out * OUT };
      };
      let x = cols[0], y = 0, fade = 1, cardX = cols[0];
      if (t >= 1 && t < 5) ({ x, y } = leg(1, 0));
      else if (t >= 5 && t < 9) ({ x, y } = leg(5, 1));
      else if (t >= 9) { const back = ease((t - 9) / 1.2); x = cols[2] + (cols[0] - cols[2]) * back; fade = Math.abs(1 - 2 * back); }
      if (t < 9) cardX = x;
      else if (fade < 1 && x > (cols[0] + cols[2]) / 2) cardX = cols[2];
      cardEl.setAttribute("transform", translateAlong(P, [cardX - cols[0], y, 0]));
      cardEl.setAttribute("opacity", fade.toFixed(2));
      pickerEl.setAttribute("transform", translateAlong(P, [x, 0, 0]));
    };
  };
