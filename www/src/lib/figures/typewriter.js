import { translateAlong, segment } from './kit/iso-kit.js';

// Starts the motion; the returned step advances it by dt seconds.
export const animate = ({ root, P, text, keys, paperX, paperY, top }) => {
    const carriage = root.querySelector('[data-part="carriage"]');
    const typed = root.querySelector('[data-part="typed"]');
    const keyEls = [...root.querySelectorAll('[data-key]')];
    const PITCH = 1.6, LEAD = 3.6;
    let line = 0, col = 0, shift = 0, wait = 0.6, pressed = null, pressT = 0, returning = false;
    const isSpace = (l, c) => !text[l].some(([a, b]) => c >= a && c < b);
    const draw = () => {
      const parts = [];
      for (let l = 0; l <= line && l < text.length; l++) {
        const z = top - l * LEAD;
        for (const [a, b] of text[l]) {
          const end = l < line ? b : Math.min(b, col);
          if (end > a) parts.push(segment([paperX + a * PITCH, paperY, z], [paperX + end * PITCH - 0.4, paperY, z], P));
        }
      }
      typed.setAttribute("d", parts.join(""));
    };
    return (dt) => {
      const target = returning ? 0 : col * PITCH;
      shift += (target - shift) * (1 - Math.exp(-dt / (returning ? 0.16 : 0.04)));
      carriage.setAttribute("transform", translateAlong(P, [-shift, 0, 0]));
      if (pressed) {
        pressT += dt;
        const down = Math.max(0, 1 - Math.abs(pressT - 0.05) / 0.05) * 1.6;
        pressed.setAttribute("transform", translateAlong(P, [0, 0, -down]));
        if (pressT > 0.1) { pressed.removeAttribute("transform"); pressed = null; }
      }
      wait -= dt;
      if (wait <= 0) {
        if (returning) {
          returning = false; line++; col = 0;
          if (line >= text.length) { line = 0; draw(); wait = 1.6; return; }
          wait = 0.3;
        } else {
          const words = text[line];
          const end = words.length ? words[words.length - 1][1] : 0;
          if (col >= end) { returning = true; wait = 0.55; }
          else {
            const space = isSpace(line, col);
            pressed = space ? keyEls.find((el) => el.dataset.key === "space") : keyEls[Math.floor(Math.random() * keys)];
            pressT = 0;
            col++;
            wait = space ? 0.16 : 0.07 + Math.random() * 0.06;
          }
        }
        draw();
      }
    };
  };
