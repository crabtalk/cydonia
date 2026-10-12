import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import * as k from '../src/lib/figures/kit/iso-kit.js';
import * as G from '../src/lib/figures/kit/lathe.js';

const at = (path) => fileURLToPath(new URL(path, import.meta.url));
const FADE_RIBS = [0.1, 0.55];

const S = (paths, style) => k.solidSvg(paths, style);
const Ln = (d, style) => k.lineSvg(d, style);
const Dots = (points, style) => k.dotsSvg(points, style);
const along = (o, a) => G.frameAlong(o, G.unit3(a));
const rod = (from, to, r, P) => {
  const d = G.sub3(to, from);
  return G.disc(0, G.len3(d), r, G.frameAlong(from, G.unit3(d)), P);
};

function basePlate(P, plan, h, foot) {
  const at = (p) => k.iso(p, P);
  const out = [`<path class="iso-halo" d="${k.haloOf(plan, -h - foot, h, P)}"/>`];
  const feet = [...k.corners(plan, 12)].sort((a, b) => k.depthOf([...a, 0], P) - k.depthOf([...b, 0], P));
  for (const [x, y] of feet) {
    out.push(S(k.cylinder(x, y, 6, -h - foot, 1.4, P, 28), { tone: "lo" }));
    out.push(S(k.cylinder(x, y, 4.6, -h - foot + 1.4, foot - 1.4, P, 28), { tone: "mid" }));
  }
  out.push(S(k.slabOf(plan, -h, h, P, 8, 1.4), { tone: "mid", inner: Ln(k.sideSeam(plan, -h + 2.4, P), { tone: "faint" }) }));
  out.push(Ln(k.planOutline(k.insetPlan(plan, 5), 0, P), { tone: "lo" }));
  const screws = k.corners(plan, 9);
  out.push(Ln(screws.map(([x, y]) => k.ring(x, y, 2, 0, P, 16)).join(""), { tone: "lo" }));
  out.push(Dots(screws.map(([x, y]) => at([x, y, 0])), { size: 0.5 }));
  return out.join("");
}

// ---- Fig 1: a rover at Cydonia ---------------------------------------------
// The rover drills a core, and every core fills the next numbered cell in its tray.

function rover() {
  const W = 640;
  const H = 440;
  const PLATE = { x: -130, y: -92, w: 260, d: 184, r: 8 };
  const P = k.fitProjection([...k.boxCorners(PLATE, -16, 0), [30, -16, 134], [-64, 0, 100], [84, 18, 60]], W, H, { pad: 30, azimuth: 40 });
  const at = (p) => k.iso(p, P);
  const V = G.viewOf(P);
  const out = [basePlate(P, PLATE, 10, 6)];

  // The ground: a ruler strip, a few rocks and the cored spot under the drill.
  const strip = { x: -118, y: PLATE.y + PLATE.d - 15, w: 120, d: 7, r: 1 };
  out.push(S(k.slabOf(strip, 0, 0.6, P, 3), { tone: "lo", crease: "none" }));
  const t = k.topTicks(strip.x + 4, strip.x + strip.w - 4, 4, 5, strip.y + 0.8, 0.6, [2, 3.6], P);
  out.push(Ln(t.minor, { tone: "lo" }), Ln(t.major, { tone: "mid" }));
  for (const [x, y, r] of [[-104, -60, 5], [-92, -70, 3], [100, -66, 6], [112, -50, 3.4], [-60, 70, 4]]) out.push(S(G.sphereOf([x, y, r * 0.35], r, P), { tone: "mid" }));
  const DRILL = [80, 18];
  out.push(Ln(k.ring(DRILL[0], DRILL[1], 5, 0, P, 24) + k.ring(DRILL[0], DRILL[1], 2.4, 0, P, 16), { tone: "lo" }));

  const items = [];
  const put = (point, svg, bias = 0) => items.push({ key: G.dot3(point, V) + bias, svg });

  const HUB_Z = 16;
  const WHEEL_R = 16;
  const wheel = (x, side) => {
    const centre = [x, 42 * side, HUB_Z];
    const F = along(centre, [0, side, 0]);
    const treads = k.fadedSvg(G.ribsOf(F, -6, 6, WHEEL_R, 30, P, { fade: FADE_RIBS }), { tone: "lo" });
    const hubFace = G.dot3(F.a, V) > 0.12;
    const hub = S(G.disc(6, 7.6, 6.4, F, P), { tone: "hi" }) + (hubFace ? Dots(G.dotsOf(F, 7.6, 4.2, 6, P, { all: true }), { size: 0.45 }) : "");
    put(centre, S(G.disc(-6, 6, WHEEL_R, F, P), { tone: "mid", inner: treads }) + (hubFace ? hub : ""));
  };
  const suspension = (side) => {
    const y = 34 * side;
    const K = [-8, y, 46];
    const B = [26, y, 30];
    const hubs = [-52, 0, 52].map((x) => [x, y, HUB_Z]);
    for (const hub of hubs) put(hub, S(rod(hub, [hub[0], 36 * side, HUB_Z], 3, P), { tone: "mid" }), -0.5);
    put(G.lerp3(hubs[0], K, 0.5), S(rod(hubs[0], K, 2.2, P), { tone: "mid" }));
    put(G.lerp3(K, B, 0.5), S(rod(K, B, 2.2, P), { tone: "mid" }));
    put(G.lerp3(hubs[1], B, 0.5), S(rod(hubs[1], B, 2.2, P), { tone: "mid" }));
    put(G.lerp3(hubs[2], B, 0.5), S(rod(hubs[2], B, 2.2, P), { tone: "mid" }));
    put(K, S(G.sphereOf(K, 3.8, P), { tone: "hi" }), 1);
    put(B, S(G.sphereOf(B, 3.4, P), { tone: "hi" }), 1);
    put([-8, 30 * side, 46], S(rod([-8, 26 * side, 46], K, 2.8, P), { tone: "mid" }), -1);
  };

  // Far side, then the body and what stands on it, then the near side and the arm.
  suspension(-1);
  for (const x of [-52, 0, 52]) wheel(x, -1);
  items.sort((a, b) => a.key - b.key);
  out.push(...items.splice(0).map((item) => item.svg));

  const BODY = { x: -42, y: -26, w: 84, d: 52, r: 4 };
  out.push(S(k.slabOf(BODY, 46, 22, P, 6, 1), { tone: "mid", inner: Ln(k.sideSeam(BODY, 52, P, 6), { tone: "faint" }) }));
  out.push(Ln(k.planOutline(k.insetPlan(BODY, 3.4), 68, P, 6), { tone: "faint" }));
  out.push(Dots(k.corners(BODY, 6).map(([x, y]) => at([x, y, 68])), { size: 0.45 }));
  out.push(Dots(k.dotGrid(-36, -20, 6, 4, 3.2, 68, P), { size: 0.4, tone: "lo" }));

  // The sample tray: two rows of six cells; each core lights the next one.
  const TRAY = { x: -30, y: 0, w: 46, d: 20, r: 1.5 };
  out.push(S(k.slabOf(TRAY, 68, 2.4, P, 3, 0.4), { tone: "hi", crease: "lo" }));
  const cells = [];
  for (let row = 0; row < 2; row++) for (let col = 0; col < 6; col++) cells.push([TRAY.x + 5 + col * 7.2, TRAY.y + 5 + row * 10]);
  out.push(Ln(cells.map(([x, y]) => k.ring(x, y, 2.6, 70.4, P, 18)).join(""), { tone: "lo" }));
  out.push(cells.map(([x, y], i) => `<circle data-cell="${i}" class="cy-core" cx="${at([x, y, 70.4])[0].toFixed(1)}" cy="${at([x, y, 70.4])[1].toFixed(1)}" r="2.2"/>`).join(""));

  // The power unit leans off the back on a bracket, finned.
  out.push(S(k.slabOf({ x: -42, y: -9, w: 12, d: 18, r: 1.5 }, 68, 8, P, 3, 0.4), { tone: "mid" }));
  const RTG = along([-38, 0, 80], [-0.8, 0, 0.6]);
  out.push(S(G.disc(0, 26, 7.5, RTG, P), { tone: "hi", inner: k.fadedSvg(G.ribsOf(RTG, 2, 24, 7.5, 18, P, { fade: FADE_RIBS }), { tone: "lo" }) }));
  out.push(S(G.disc(26, 28, 8.4, RTG, P), { tone: "mid" }));

  // The mast and its camera head.
  out.push(S(k.cylinder(30, -16, 5, 68, 3, P, 28), { tone: "mid" }));
  out.push(S(k.cylinder(30, -16, 2.6, 71, 49, P, 20), { tone: "mid", crease: "none", inner: Ln(k.sideArc(30, -16, 2.6, 96, P, 12), { tone: "faint" }) }));
  const HEAD = { x: 22, y: -26, w: 18, d: 20, r: 2 };
  out.push(S(k.slabOf(HEAD, 120, 11, P, 4, 0.6), { tone: "hi", crease: "lo" }));
  out.push(Ln(k.sideRing([HEAD.x + HEAD.w + 0.1, -20, 125.5], 2.6, "x", P) + k.sideRing([HEAD.x + HEAD.w + 0.1, -12, 125.5], 2.6, "x", P), { tone: "mid" }));
  out.push(Dots([at([HEAD.x + HEAD.w + 0.1, -20, 125.5]), at([HEAD.x + HEAD.w + 0.1, -12, 125.5])], { size: 0.6, tone: "hi" }));

  suspension(1);
  for (const x of [-52, 0, 52]) wheel(x, 1);

  // The arm: shoulder on the body's front, elbow, and a turret carrying the drill.
  const SHOULDER = [44, 18, 54];
  const ELBOW = [66, 18, 64];
  const WRIST = [80, 18, 44];
  put(SHOULDER, S(rod([42, 18, 54], SHOULDER, 4.2, P), { tone: "mid" }));
  put(G.lerp3(SHOULDER, ELBOW, 0.5), S(rod(SHOULDER, ELBOW, 2.6, P), { tone: "mid" }));
  put(ELBOW, S(G.sphereOf(ELBOW, 3.6, P), { tone: "hi" }), 1);
  put(G.lerp3(ELBOW, WRIST, 0.5), S(rod(ELBOW, WRIST, 2.4, P), { tone: "mid" }));
  put(WRIST, S(k.cylinder(80, 18, 6, 38, 8, P, 32, 0.5), { tone: "hi", crease: "lo" }) + Ln(k.knurl(80, 18, 6, 39, 45, 30, P), { tone: "lo" }), 2);
  items.sort((a, b) => a.key - b.key);
  out.push(...items.map((item) => item.svg));

  // The drill bit hangs from the turret and moves on its own.
  const bit = S(k.cylinder(80, 18, 3, 34, 4, P, 20), { tone: "mid" }) + S(k.cylinder(80, 18, 1.5, 10, 24, P, 16), { tone: "hi", lit: true, crease: "none" });
  const flutes = Ln([14, 18, 22, 26, 30].map((z) => k.segment([80 - 1.5, 18, z - 2], [80 + 1.5, 18, z], P)).join(""), { tone: "lo" });
  out.push(`<g data-part="bit">${bit}${flutes}</g>`);
  const dust = [];
  for (let i = 0; i < 10; i++) {
    const a = (i / 10) * Math.PI * 2;
    dust.push(at([80 + Math.cos(a) * (4 + (i % 3)), 18 + Math.sin(a) * (4 + (i % 3)), 0.4]));
  }
  out.push(`<g data-part="dust" opacity="0">${Dots(dust, { size: 0.6, tone: "mid" })}</g>`);

  const svg = k.figureSvg({
    width: W,
    height: H,
    label: "A six-wheeled rover on a slab of Martian ground: rocker-bogie suspension, a body with a sample tray, a finned power unit, a camera mast and an arm whose drill takes a core from the ground and files it in the next numbered cell of the tray.",
    body: out,
  });
  return {
    id: "rover",
    svg,
    data: { P, cells: cells.length },
  };
}

// ---- Fig 2: a typewriter ----------------------------------------------------
// The carriage steps left one character at a time and returns at the end of a line;
// the page fills line by line.

function typewriter() {
  const W = 640;
  const H = 440;
  const PLATE = { x: -96, y: -70, w: 192, d: 140, r: 8 };
  const P = k.fitProjection([...k.boxCorners(PLATE, -14, 0), [-80, -46, 100], [80, -46, 100]], W, H, { pad: 30, azimuth: 56 });
  const at = (p) => k.iso(p, P);
  const out = [basePlate(P, PLATE, 8, 6)];

  const CASE = { x: -62, y: -46, w: 124, d: 84, r: 7 };
  out.push(`<path class="iso-halo" d="${k.haloOf(k.insetPlan(CASE, 4), 0, 0.1, P)}" style="filter:blur(4px)"/>`);
  for (const [x, y] of k.corners(CASE, 9)) out.push(S(k.cylinder(x, y, 4.2, 0, 2.4, P, 20), { tone: "lo" }));
  out.push(S(k.slabOf(CASE, 2.4, 13, P, 8, 1), { tone: "mid", inner: Ln(k.sideSeam(CASE, 6, P), { tone: "faint" }) }));
  const HOUSING = { x: -58, y: -44, w: 116, d: 34, r: 5 };
  out.push(S(k.slabOf(HOUSING, 15.4, 22, P, 6, 0.8), { tone: "mid" }));
  out.push(Ln(k.planOutline(k.insetPlan(HOUSING, 3), 37.4, P, 6), { tone: "faint" }));
  out.push(k.faceTextSvg(k.sideMatrix([0, HOUSING.y + HOUSING.d + 0.1, 26], 26, P, "left"), "CYDONIA", { size: 3.2, tone: "lo", anchor: "middle" }));

  // Ribbon spools on the housing, joined by the ribbon.
  for (const x of [-38, 38]) {
    out.push(S(k.cylinder(x, -30, 8, 37.4, 4, P, 36, 0.5), { tone: "hi", crease: "lo" }));
    out.push(Ln(k.ring(x, -30, 2.4, 41.4, P, 16), { tone: "lo" }));
  }
  out.push(Ln(k.lineOnTop([-38, -22], [38, -22], 41.4, P), { tone: "mid" }));

  // The carriage rail and the carriage: platen, knobs, paper and the typed lines.
  const RAIL = { x: -66, y: -48, w: 132, d: 6, r: 1.5 };
  out.push(S(k.slabOf(RAIL, 37.4, 4, P, 3, 0.4), { tone: "mid" }));
  const PLATEN = along([0, -42, 49], [1, 0, 0]);
  const PAPER_Y = -45.5;
  const PAPER = { x: -30, y: PAPER_Y, w: 60, d: 0.6, r: 0 };
  const carriage = [];
  carriage.push(S(G.disc(-58, 58, 7, PLATEN, P), { tone: "hi", crease: "lo" }));
  for (const side of [-1, 1]) {
    const F = along([58 * side, -42, 49], [side, 0, 0]);
    carriage.push(S(G.disc(0, 7, 6, F, P), { tone: "mid", inner: k.fadedSvg(G.ribsOf(F, 0, 7, 6, 24, P, { fade: FADE_RIBS }), { tone: "lo" }) }));
  }
  carriage.push(S(rod([-62, -42, 56], [-74, -36, 64], 1.2, P), { tone: "mid" }));
  carriage.push(S(G.sphereOf([-74, -36, 64], 2.2, P), { tone: "hi" }));
  carriage.push(S(k.slabOf(PAPER, 56, 42, P, 2), { tone: "hi", crease: "none", lit: true }));
  carriage.push(`<path data-part="typed" class="iso-line" data-tone="hi" data-free="true" d=""/>`);
  out.push(`<g data-part="carriage">${carriage.join("")}</g>`);
  out.push(S(k.slabOf({ x: -4, y: -36, w: 8, d: 4, r: 1 }, 41.4, 8, P, 2, 0.3), { tone: "mid" }));
  out.push(Dots([at([0, -34, 49.4])], { size: 0.6, tone: "hi" }));

  // Four stepped rows of keys, back row first, and the space bar.
  const ROWS = 4;
  const keys = [];
  for (let row = ROWS - 1; row >= 0; row--) {
    const y = 28 - row * 9;
    const z = 15.4 + row * 2.6;
    out.push(S(k.slabOf({ x: -56, y: y - 4.5, w: 112, d: 9, r: 1.5 }, 15.4, z - 15.4 + 0.01, P, 3), { tone: "mid", crease: "none" }));
    for (let col = 0; col < 10; col++) {
      const x = -46 + col * 10 + row * 2.4;
      const index = keys.length;
      keys.push(index);
      const key = S(k.cylinder(x, y, 0.9, z, 3, P, 12), { tone: "lo" }) + S(k.cylinder(x, y, 3.4, z + 3, 1.6, P, 24, 0.3), { tone: "hi", crease: "lo" }) + Ln(k.ring(x, y, 2.4, z + 4.6, P, 18), { tone: "faint" });
      out.push(`<g data-key="${index}">${key}</g>`);
    }
  }
  out.push(`<g data-key="space">${S(k.slabOf({ x: -26, y: 33, w: 52, d: 4.4, r: 1.8 }, 17, 2.4, P, 3, 0.3), { tone: "hi", crease: "lo" })}</g>`);

  // The text on the page, as word lengths per line, so the live code draws dashes like type.
  const COLS = 34;
  const LINES = 10;
  let seed = 7;
  const rand = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  const text = [];
  for (let line = 0; line < LINES; line++) {
    const words = [];
    let col = line === 0 ? 3 : 0;
    const end = line === LINES - 1 ? 18 : COLS - Math.floor(rand() * 5);
    while (col < end - 2) {
      const len = Math.min(end - col, 2 + Math.floor(rand() * 7));
      words.push([col, col + len]);
      col += len + 1;
    }
    text.push(words);
  }

  const svg = k.figureSvg({
    width: W,
    height: H,
    label: "A typewriter on a plate: four stepped rows of keys and a space bar, ribbon spools on the housing, and a carriage with a platen, knobs and a sheet of paper that steps left as each character is typed and returns at the end of the line.",
    body: out,
  });
  return {
    id: "typewriter",
    svg,
    data: { P, text, keys: keys.length, paperX: PAPER.x + 3, paperY: PAPER_Y + 0.7, top: 94 },
  };
}

// ---- Fig 3: a pigeonhole sorting rack ----------------------------------------
// Three columns of pigeonholes; a picker on a rail draws a card out of one column and files it in the next.

function rack() {
  const W = 640;
  const H = 460;
  const PLATE = { x: -92, y: -54, w: 184, d: 104, r: 8 };
  const P = k.fitProjection([...k.boxCorners(PLATE, -14, 0), [-68, -33, 108], [68, -33, 108], [68, 30, 110]], W, H, { pad: 30, azimuth: 52 });
  const at = (p) => k.iso(p, P);
  const out = [basePlate(P, PLATE, 8, 6)];

  const COLS = [-43, 0, 43];
  const WALLS = [-64.5, -21.5, 21.5, 64.5];
  const SHELVES = [6, 37, 68, 99];
  const SHELF_H = 3;
  const RACK_X = [-66, 66];
  const DEPTH = 30;
  // Cards resting in each cell, [column][row], the bottom row first.
  const COUNT = [
    [2, 3, 1],
    [1, 0, 1],
    [3, 0, 2],
  ];
  const ROW = 1;

  out.push(S(k.slabOf({ x: -68, y: -34, w: 136, d: 37, r: 2 }, 0, 6, P, 3, 0.6), { tone: "lo" }));
  out.push(S(k.slabOf({ x: RACK_X[0], y: -DEPTH - 2, w: RACK_X[1] - RACK_X[0], d: 2, r: 0.5 }, 6, SHELVES[3] + SHELF_H - 6, P, 2), { tone: "mid" }));

  const card = (cx, y, z, { lit = false, tab = 0 } = {}) => {
    const plan = { x: cx - 15, y, w: 30, d: 0.7, r: 0.3 };
    const tabPlan = { x: cx - 15 + 3 + tab * 9, y, w: 8, d: 0.7, r: 0.3 };
    const face = y + 0.75;
    const rules = [z + 16, z + 12.5, z + 9].map((h, i) => k.segment([cx - 11, face, h], [cx + (i === 2 ? 2 : 11), face, h], P)).join("");
    return S(k.slabOf(plan, z, 21, P, 2), { tone: "hi", crease: "lo", lit }) + S(k.slabOf(tabPlan, z + 21, 3, P, 2), { tone: "hi", crease: "lo", lit }) + Ln(rules, { tone: lit ? "mid" : "faint", free: true });
  };
  const shelf = (z) => {
    const plan = { x: RACK_X[0], y: -DEPTH, w: RACK_X[1] - RACK_X[0], d: DEPTH, r: 0.5 };
    return S(k.slabOf(plan, z, SHELF_H, P, 2, 0.4), { tone: "mid" });
  };

  for (let row = 0; row < 3; row++) {
    const floor = SHELVES[row] + SHELF_H;
    const ceiling = SHELVES[row + 1];
    out.push(shelf(SHELVES[row]));
    WALLS.forEach((x, w) => {
      out.push(S(k.slabOf({ x: x - 1.5, y: -DEPTH, w: 3, d: DEPTH, r: 0.3 }, floor, ceiling - floor, P, 2), { tone: "mid", crease: "none" }));
      if (w === 3) return;
      const n = COUNT[w][row];
      for (let i = n - 1; i >= 0; i--) out.push(card(COLS[w], -1.6 - i * 3, floor, { tab: (w + row + i) % 3 }));
    });
  }
  out.push(shelf(SHELVES[3]));
  // Column plates on the top shelf's front edge.
  COLS.forEach((cx) => {
    const plate = { x: cx - 9, y: -1.2, w: 18, d: 1.2, r: 0.3 };
    out.push(S(k.slabOf(plate, SHELVES[3] + SHELF_H, 4, P, 2), { tone: "lo", crease: "none" }));
  });
  for (const x of WALLS) out.push(Dots([at([x, -0.4, SHELVES[3] + SHELF_H])], { size: 0.5 }));

  // The card on the move, then the picker in front of the rack.
  const LIFT = SHELVES[ROW] + SHELF_H;
  out.push(`<g data-part="card">${card(COLS[0], -1.6 + 3, LIFT, { lit: true, tab: 1 })}</g>`);
  out.push(S(k.slabOf({ x: -76, y: 20, w: 152, d: 9, r: 1.5 }, 0, 3, P, 3, 0.4), { tone: "mid" }));
  const railTicks = k.topTicks(-72, 72, 4, 5, 20.8, 3, [1.6, 3], P);
  out.push(Ln(railTicks.minor, { tone: "lo" }), Ln(railTicks.major, { tone: "mid" }));
  for (const x of [-74, 74]) out.push(S(k.slabOf({ x: x - 2, y: 19, w: 4, d: 11, r: 0.8 }, 3, 5, P, 2), { tone: "mid" }));

  const HEAD_Z = LIFT + 6;
  const MX = 0;
  const picker = [];
  picker.push(S(k.slabOf({ x: MX - 9, y: 18.5, w: 18, d: 12, r: 2 }, 3, 6, P, 3, 0.5), { tone: "mid" }));
  picker.push(Dots([at([MX - 6, 21, 9]), at([MX + 6, 21, 9])], { size: 0.45 }));
  picker.push(S(k.cylinder(MX, 26, 2.4, 9, HEAD_Z - 9, P, 20), { tone: "mid", crease: "none" }));
  picker.push(S(k.slabOf({ x: MX - 4, y: 17.2, w: 2.2, d: 4, r: 0.4 }, HEAD_Z + 2, 8, P, 2), { tone: "mid" }));
  picker.push(S(k.slabOf({ x: MX + 1.8, y: 17.2, w: 2.2, d: 4, r: 0.4 }, HEAD_Z + 2, 8, P, 2), { tone: "mid" }));
  picker.push(S(k.slabOf({ x: MX - 7, y: 20.5, w: 14, d: 10, r: 1.5 }, HEAD_Z, 12, P, 3, 0.5), { tone: "hi", crease: "lo" }));
  picker.push(Ln(k.sideSeam({ x: MX - 7, y: 20.5, w: 14, d: 10, r: 1.5 }, HEAD_Z + 4, P, 3), { tone: "faint" }));
  picker.push(S(k.cylinder(MX, 26, 2.4, HEAD_Z + 12, 104 - HEAD_Z - 12, P, 20), { tone: "mid", crease: "none" }));
  picker.push(S(k.cylinder(MX, 26, 3.4, 104, 2, P, 24), { tone: "mid" }));
  out.push(`<g data-part="picker">${picker.join("")}</g>`);

  const svg = k.figureSvg({
    width: W,
    height: H,
    label: "A pigeonhole rack of three columns and three rows holding index cards, with a picker on a rail in front that draws one card out of the To do column and files it under Doing, then Done.",
    body: out,
  });
  return {
    id: "rack",
    svg,
    data: { P, cols: COLS },
  };
}

// ---- Fig 4: a card-index cabinet ---------------------------------------------
// Six numbered drawers of index cards; one at a time slides out and a card rises.

function cabinet() {
  const W = 640;
  const H = 460;
  const PLATE = { x: -84, y: -60, w: 168, d: 128, r: 8 };
  const P = k.fitProjection([...k.boxCorners(PLATE, -14, 0), [-56, -40, 98], [56, -40, 98], [30, 52, 40]], W, H, { pad: 30, azimuth: 50 });
  const at = (p) => k.iso(p, P);
  const out = [basePlate(P, PLATE, 8, 6)];

  const FRONT = 16;
  const COLS = [-25.5, 25.5];
  const ROWS = [65, 37, 9];
  const PANEL_H = 25;
  const PER = 40;

  out.push(S(k.slabOf({ x: -48, y: -32, w: 96, d: 46, r: 2 }, 0, 6, P, 3), { tone: "lo" }));
  const BODY = { x: -52, y: -36, w: 104, d: FRONT + 36, r: 2 };
  out.push(S(k.slabOf(BODY, 6, 88, P, 3, 0.6), { tone: "mid", inner: Ln(k.sideSeam(BODY, 62.5, P, 3) + k.sideSeam(BODY, 34.5, P, 3), { tone: "faint" }) }));
  out.push(S(k.slabOf({ x: -54, y: -38, w: 108, d: FRONT + 40, r: 3 }, 94, 3, P, 4, 0.8), { tone: "mid" }));
  out.push(Ln(k.planOutline({ x: -50, y: -34, w: 100, d: FRONT + 32, r: 2 }, 97, P, 3), { tone: "faint" }));
  out.push(Ln(k.segment([0, FRONT, 6], [0, FRONT, 94], P), { tone: "faint" }));

  const drawers = [];
  const panel = (cx, z0, number) => {
    const parts = [];
    parts.push(S(k.slabOf({ x: cx - 23, y: FRONT, w: 46, d: 2, r: 1 }, z0, PANEL_H, P, 3, 0.4), { tone: "hi", crease: "lo" }));
    const face = FRONT + 2.05;
    const rect = [[cx - 9, face, z0 + 15], [cx + 9, face, z0 + 15], [cx + 9, face, z0 + 21.5], [cx - 9, face, z0 + 21.5]];
    parts.push(`<path class="iso-line" data-tone="lo" d="${k.pathOf(rect.map(at), true)}"/>`);
    parts.push(Dots([at([cx - 10.6, face, z0 + 18.2]), at([cx + 10.6, face, z0 + 18.2])], { size: 0.42 }));
    parts.push(k.faceTextSvg(k.sideMatrix([cx, face], z0 + 19.6, P, "left"), `${number * PER + 1}–${(number + 1) * PER}`, { size: 3, tone: "mid", anchor: "middle" }));
    const F = along([cx, FRONT + 2, z0 + 9], [0, 1, 0]);
    parts.push(S(G.disc(0, 2.2, 1.2, F, P), { tone: "mid" }));
    parts.push(S(G.disc(2.2, 3.6, 3.2, F, P), { tone: "hi", crease: "lo" }));
    parts.push(Ln(G.circleOf(F, 3.6, 1.6, P), { tone: "faint" }));
    return parts.join("");
  };
  let number = 0;
  for (const z0 of ROWS) {
    for (const cx of COLS) {
      drawers.push({ cx, z0, number });
      out.push(`<g data-drawer="${number}">${panel(cx, z0, number)}</g>`);
      number++;
    }
  }
  out.push(`<g data-part="box"></g><g data-part="front"></g>`);

  const svg = k.figureSvg({
    width: W,
    height: H,
    label: "A card-index cabinet of six numbered drawers, two across and three high, on a plinth. One drawer at a time slides out to show its index cards, and one card rises out of it.",
    body: out,
  });
  return {
    id: "cabinet",
    svg,
    data: { P, drawers, front: FRONT },
  };
}

// ---- Output -----------------------------------------------------------------

/** Writes each figure's SVG to static/figures/, and the data its motion needs and the kit's styles to src/lib/generated/figures/. */
export async function generateFigures() {
	const dir = at('../src/lib/generated/figures/');
	const drawings = at('../static/figures/');
	await mkdir(dir, { recursive: true });
	await mkdir(drawings, { recursive: true });
	for (const figure of [rover(), typewriter(), rack(), cabinet()]) {
		const { svg, data, id } = figure;
		await writeFile(`${drawings}${id}.svg`, svg);
		await writeFile(`${dir}${id}.json`, JSON.stringify(data));
	}
	await writeFile(`${dir}iso.css`, k.ISO_CSS);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await generateFigures();
