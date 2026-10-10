// The page draws what the core returns and decides nothing. It loads the data
// files, starts the core through the shim, and each display frame passes the
// clock, the held actions and the mouse movement in, then draws what comes
// out. The core says which mode the game is in; the page shows that screen.
import init, { Game, numbers } from "./pkg/pursuer_shim.js";
import { makeRenderer } from "./gl.js";
import { makeInput } from "./input.js";
import { makeHud } from "./hud.js";
import { setup as setupSound, wake, toggleMute, play, musicOn } from "./sound.js";

const BUILD = "__BUILD__";
const SAVE_KEY = "pursuer.record";
const el = (id) => document.getElementById(id);
const file = (path) => fetch(`${path}?v=${BUILD}`).then((r) => r.text());

// Browser storage can be missing or refused; the game works without it.
const storage = {
  load() {
    try {
      return localStorage.getItem(SAVE_KEY) || "";
    } catch {
      return "";
    }
  },
  save(text) {
    try {
      localStorage.setItem(SAVE_KEY, text);
    } catch {
      // nothing to do: the record is simply not kept
    }
  },
};

function fillCopy(copy) {
  for (const node of document.querySelectorAll("[data-copy]")) {
    node.textContent = copy[node.dataset.copy];
  }
  document.title = copy.title;
  el("build").textContent = BUILD;
  for (const table of document.querySelectorAll("table.controls")) {
    for (const [keys, does] of copy.controls) {
      const row = table.insertRow();
      row.insertCell().textContent = keys;
      row.insertCell().textContent = does;
    }
  }
}

function loadImage(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error(`could not load ${src}`));
    img.src = `${src}?v=${BUILD}`;
  });
}

async function start() {
  const names = ["palette", "items", "upgrades", "pursuer", "copy", "controls", "sounds", "music", "display"];
  const texts = await Promise.all(
    names.map((n) => file(`data/${n}.json`)).concat([file("data/shaders/scene.vert"), file("data/shaders/scene.frag")]),
  );
  const [palette, items, upgrades, pursuer, copyText, controlsText, soundsText, musicText, displayText, vertex, fragment] = texts;
  const copy = JSON.parse(copyText);
  const display = JSON.parse(displayText);
  fillCopy(copy);
  setupSound(JSON.parse(soundsText), JSON.parse(musicText));
  const wasm = await init();
  const N = JSON.parse(numbers());

  let game;
  try {
    game = new Game(palette, items, upgrades, pursuer, musicText, storage.load());
  } catch (e) {
    el("message").textContent = String(e);
    document.body.dataset.ready = "true";
    return;
  }

  const itemList = JSON.parse(game.items());
  // every item's pursuer, in order, then each cover, in order: the core's texture numbers
  const pictures = itemList
    .map((it) => `art/monsters/${it.monster}.png`)
    .concat(itemList.filter((it) => it.cover).map((it) => `art/covers/${it.cover}.png`));
  const images = await Promise.all(pictures.map(loadImage));
  const canvas = el("view");
  const renderer = makeRenderer(canvas, N, { vertex, fragment, software: display.software }, (id) => game.mesh(id), images);
  if (!renderer) el("message").textContent = copy.no_webgl;
  game.set_low_detail(Boolean(renderer && renderer.software));
  const save = () => storage.save(game.saved());
  const input = makeInput(canvas, JSON.parse(controlsText), N.actions, () => wake());
  const hud = makeHud(copy);
  let shownMode = null;

  const act = (fn) => () => {
    wake();
    fn();
    if (document.activeElement) document.activeElement.blur();
  };
  const engage = () => input.engage();
  el("begin").addEventListener(
    "click",
    act(() => {
      game.begin();
      engage();
    }),
  );
  el("engage-btn").addEventListener("click", act(engage));
  el("resume").addEventListener(
    "click",
    act(() => {
      game.resume();
      engage();
    }),
  );
  el("quit").addEventListener("click", act(() => game.to_title()));
  el("again").addEventListener(
    "click",
    act(() => {
      game.again();
      engage();
    }),
  );
  el("to-title").addEventListener("click", act(() => game.to_title()));

  // losing a pointer lock that was held pauses play
  let held = false;
  document.addEventListener("pointerlockchange", () => {
    const now = Boolean(document.pointerLockElement);
    const mode = document.body.dataset.mode;
    if (held && !now && (mode === "house" || mode === "chase")) game.pause();
    held = now;
  });
  window.addEventListener("keydown", (e) => {
    const mode = document.body.dataset.mode;
    if (e.code === "Escape" && (mode === "house" || mode === "chase")) game.pause();
    if (e.code === "KeyN") toggleMute();
    wake();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) game.pause();
  });

  function resize() {
    const scale = renderer && renderer.software ? display.software_scale : 1;
    const [w, h] = game.resize(canvas.clientWidth, canvas.clientHeight, (window.devicePixelRatio || 1) * scale);
    if (renderer) renderer.resize(w, h);
  }
  window.addEventListener("resize", resize);
  resize();

  const overlays = { title: "title", paused: "paused", caught: "caught" };
  function showMode(h) {
    if (h.mode === shownMode) return;
    document.body.dataset.mode = h.mode;
    for (const [m, id] of Object.entries(overlays)) el(id).hidden = h.mode !== m;
    el("hud").hidden = !["house", "chase", "paused"].includes(h.mode);
    if (h.mode === "title") el("title-best").textContent = h.best;
    if (h.mode === "caught") {
      el("c-monster").textContent = h.monster;
      el("c-line").textContent = h.line;
      el("c-time").textContent = h.time;
      el("c-best").textContent = h.best;
      el("c-new").hidden = !h.new_best;
      el("c-loosed").textContent = h.loosed;
      el("c-art").src = `art/monsters/${h.monster_id}.png?v=${BUILD}`;
      save();
    }
    if (h.mode === "title" || h.mode === "caught" || h.mode === "paused") input.release();
    shownMode = h.mode;
  }

  let piecesVersion = null;
  let sceneSet = false;
  function frame(now) {
    const { bits, dx, dy } = input.read();
    game.advance(now, bits, dx, dy);
    const h = JSON.parse(game.hud());
    showMode(h);
    if (h.mode === "house" || h.mode === "chase") {
      hud.update(h);
      el("engage").hidden = input.engaged();
    }
    for (const s of h.sounds) play(s);
    musicOn(h.music, h.song_id);
    if (h.sounds.includes("needle")) save();
    if (renderer) {
      if (!sceneSet) {
        renderer.setScene(game.scenery(), game.uniforms(), game.clear_color());
        sceneSet = true;
      }
      const dirty = game.take_dirty();
      // a view of the module's memory, made after every call that could grow it
      const pieces = new Int32Array(wasm.memory.buffer, game.pieces_ptr(), game.pieces_len());
      if (game.pieces_version() !== piecesVersion) {
        piecesVersion = game.pieces_version();
        renderer.setPieces(pieces);
      } else {
        renderer.updatePieces(pieces, dirty);
      }
      renderer.frame(game.rects(), game.mats(), game.draws(), game.dynamic());
    }
    requestAnimationFrame(frame);
  }

  // #everything knocks the whole city loose at once: the worst case
  if (location.hash === "#everything") game.wreck_all();
  document.body.dataset.ready = "true";
  requestAnimationFrame(frame);
}

start();
