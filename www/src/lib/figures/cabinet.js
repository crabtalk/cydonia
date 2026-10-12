import { translateAlong, slabOf, solidSvg } from './kit/iso-kit.js';

// Starts the motion; the returned step advances it by dt seconds.
export const animate = ({ root, P, drawers, front }) => {
    const boxEl = root.querySelector('[data-part="box"]');
    const frontEl = root.querySelector('[data-part="front"]');
    const panels = [...root.querySelectorAll('[data-drawer]')].sort((x, y) => Number(x.dataset.drawer) - Number(y.dataset.drawer));
    for (const el of panels) { el.removeAttribute("transform"); root.querySelector("svg").insertBefore(el, boxEl); }
    const ease = (x) => (x < 0 ? 0 : x > 1 ? 1 : x * x * (3 - 2 * x));
    const S = (paths, style) => solidSvg(paths, style);
    const CYCLE = 5;
    const PULL = 30;
    let clock = 0, active = -1, shown = "";
    const visit = [4, 1, 2, 5, 0, 3];
    return (dt) => {
      clock += dt;
      const index = visit[Math.floor(clock / CYCLE) % visit.length];
      const t = clock % CYCLE;
      if (index !== active) {
        if (active >= 0) root.querySelector("svg").insertBefore(panels[active], boxEl);
        active = index;
        frontEl.appendChild(panels[active]);
      }
      const d = drawers[active];
      const pull = PULL * (ease(t / 1) - ease((t - 3.8) / 1));
      const lift = 9 * (ease((t - 1.1) / 0.6) - ease((t - 3.0) / 0.6));
      const parts = [];
      if (pull > 1) {
        const slab = (x, w, z, h) => S(slabOf({ x, y: front, w, d: pull, r: 0 }, z, h, P, 2), { tone: "mid", crease: "none" });
        parts.push(slab(d.cx - 21, 42, d.z0 + 1, 1));
        parts.push(slab(d.cx - 21, 1.2, d.z0 + 2, 16));
        const cards = [];
        for (let y = front + 3; y < front + pull - 1.6; y += 3.2) cards.push(y);
        cards.forEach((y, i) => {
          const picked = i === 2;
          const z = d.z0 + 2 + (picked ? lift : 0);
          parts.push(S(slabOf({ x: d.cx - 18, y, w: 36, d: 0.5, r: 0 }, z, 15, P, 2), { tone: "hi", crease: "lo", lit: picked }));
          parts.push(S(slabOf({ x: d.cx - 16 + (i % 3) * 12, y, w: 8, d: 0.5, r: 0 }, z + 15, 2.6, P, 2), { tone: "hi", crease: "lo", lit: picked }));
        });
        parts.push(slab(d.cx + 19.8, 1.2, d.z0 + 2, 16));
      }
      const html = parts.join("");
      if (html !== shown) { boxEl.innerHTML = html; shown = html; }
      panels[active].setAttribute("transform", translateAlong(P, [0, pull, 0]));
    };
  };
