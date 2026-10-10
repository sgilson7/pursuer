// Sound, synthesized from data/sounds.json, and the music from
// data/music.json. The core says what happened, whether the music plays and
// which record is on; this file only performs it. White noise comes from the core's generator,
// since the page may not draw random numbers.
import { noise } from "./pkg/pursuer_shim.js";

let ctx = null;
let master = null;
let noiseBuffer = null;
let recipes = null;
let music = null;
let muted = false;
const lastAt = new Map();
// the music: a recording on a loop, or the tune scheduled note by note
const tune = { playing: null, audio: null, voices: [], drone: [], next: 0, index: 0, gain: null };

export function setup(sounds, musicData) {
  recipes = sounds;
  music = musicData;
}

// Browsers start audio only after a person presses something.
export function wake() {
  if (!recipes) return;
  if (!ctx) {
    const AC = window.AudioContext || window.webkitAudioContext;
    if (!AC) return;
    ctx = new AC({ latencyHint: "interactive" });
    master = ctx.createGain();
    master.gain.value = recipes.master;
    master.connect(ctx.destination);
    const data = noise(ctx.sampleRate * recipes.noise_seconds, recipes.seed);
    noiseBuffer = ctx.createBuffer(1, data.length, ctx.sampleRate);
    noiseBuffer.copyToChannel(data, 0);
  }
  if (ctx.state === "suspended") ctx.resume().catch(() => {});
}

export function toggleMute() {
  muted = !muted;
  if (master) master.gain.value = muted ? 0 : recipes.master;
  if (tune.audio) tune.audio.muted = muted;
  return muted;
}

function layer(l, t) {
  const t0 = t + l.at;
  const g = ctx.createGain();
  g.gain.setValueAtTime(recipes.floor, t0);
  g.gain.linearRampToValueAtTime(l.gain, t0 + l.attack);
  g.gain.exponentialRampToValueAtTime(recipes.floor, t0 + l.dur);
  g.connect(master);
  let src;
  if (l.type === "osc") {
    src = ctx.createOscillator();
    src.type = l.wave;
    src.frequency.setValueAtTime(l.from, t0);
    src.frequency.exponentialRampToValueAtTime(l.to, t0 + l.dur);
    src.connect(g);
  } else {
    src = ctx.createBufferSource();
    src.buffer = noiseBuffer;
    src.loop = true;
    const f = ctx.createBiquadFilter();
    f.type = l.filter;
    f.frequency.value = l.freq;
    f.Q.value = l.q;
    src.connect(f);
    f.connect(g);
  }
  src.start(t0);
  src.stop(t0 + l.dur + recipes.tail);
}

// A sound by name. A sound asked for again sooner than its `gap` is dropped.
export function play(kind) {
  if (!ctx || muted || !recipes) return;
  const recipe = recipes.recipes[kind];
  if (!recipe) return;
  const now = ctx.currentTime;
  const gap = recipes.gaps[kind] || recipes.gap;
  if (now - (lastAt.get(kind) || -gap) < gap) return;
  lastAt.set(kind, now);
  for (const l of recipe) layer(l, now);
}

// One note of the tune: a sine with a slow vibrato, like a voice humming.
function note(freq, t, dur) {
  if (!freq) return;
  const osc = ctx.createOscillator();
  osc.type = "sine";
  osc.frequency.value = freq;
  const lfo = ctx.createOscillator();
  lfo.frequency.value = music.vibrato_hz;
  const depth = ctx.createGain();
  depth.gain.value = music.vibrato_depth;
  lfo.connect(depth);
  depth.connect(osc.frequency);
  const g = ctx.createGain();
  g.gain.setValueAtTime(recipes.floor, t);
  g.gain.linearRampToValueAtTime(music.gain, t + music.attack);
  g.gain.setValueAtTime(music.gain, t + dur);
  g.gain.exponentialRampToValueAtTime(recipes.floor, t + dur + music.release);
  osc.connect(g);
  g.connect(tune.gain);
  osc.start(t);
  lfo.start(t);
  osc.stop(t + dur + music.release + recipes.tail);
  lfo.stop(t + dur + music.release + recipes.tail);
  tune.voices.push(osc, lfo);
}

// A recording plays from its `start` and, at its end, from `start` again.
function startRecording(track) {
  const audio = new Audio(track.file);
  audio.volume = music.volume;
  audio.muted = muted;
  audio.playbackRate = track.rate;
  audio.preservesPitch = false;
  const fromStart = () => {
    audio.currentTime = track.start;
    audio.play().catch(() => {});
  };
  audio.addEventListener("loadedmetadata", fromStart, { once: true });
  audio.addEventListener("ended", fromStart);
  tune.audio = audio;
}

function startMusic(id) {
  tune.playing = id;
  const track = music.tracks.find((t) => t.id === id);
  if (track && track.file) {
    startRecording(track);
    return;
  }
  tune.gain = ctx.createGain();
  tune.gain.gain.value = music.volume;
  tune.gain.connect(master);
  for (const f of music.drone) {
    const d = ctx.createOscillator();
    d.type = "triangle";
    d.frequency.value = f;
    const g = ctx.createGain();
    g.gain.value = music.drone_gain;
    d.connect(g);
    g.connect(tune.gain);
    d.start();
    tune.drone.push(d);
  }
  tune.next = ctx.currentTime;
  tune.index = 0;
}

function stopMusic() {
  tune.playing = null;
  if (tune.audio) {
    tune.audio.pause();
    tune.audio.removeAttribute("src");
    tune.audio = null;
  }
  for (const v of tune.voices.concat(tune.drone)) {
    try {
      v.stop();
    } catch {
      // already stopped
    }
  }
  tune.voices = [];
  tune.drone = [];
  if (tune.gain) tune.gain.disconnect();
  tune.gain = null;
}

// Called each frame with whether the core wants the music and which record
// is on: keeps the tune scheduled a note ahead of the clock.
export function musicOn(on, id) {
  if (!ctx || !music) return;
  if (tune.playing && (!on || tune.playing !== id)) stopMusic();
  if (on && !tune.playing) startMusic(id);
  if (!tune.playing || tune.audio) return;
  while (tune.next < ctx.currentTime + music.notes[tune.index][1]) {
    const [freq, dur] = music.notes[tune.index];
    note(freq, tune.next, dur);
    tune.next += dur;
    tune.index = (tune.index + 1) % music.notes.length;
    tune.voices = tune.voices.slice(-music.keep_voices);
  }
}

window.addEventListener("pagehide", () => {
  if (ctx) {
    const c = ctx;
    ctx = null;
    c.close().catch(() => {});
  }
});
