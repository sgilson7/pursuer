// Touch controls, for a tablet or a phone: a stick under the left thumb, a
// look pad under the right, and buttons, turned into the same action bits and
// mouse movement the keyboard and mouse give. data/touch.json holds every
// size; the core's numbers() gives each action its bit; the core's hud()
// says whether you are in the car and which abilities the car has. Nothing
// here decides what an action does.

const el = (id) => document.getElementById(id);

// Keeps a finger's moves coming to where it went down, even off the element;
// a pointer the browser does not know (a synthetic one) is simply not captured.
function capture(node, e) {
  try {
    node.setPointerCapture(e.pointerId);
  } catch {
    // nothing to capture
  }
}

export function makeTouch(touch, actions, copy) {
  const state = { on: false, held: 0, tapped: 0, dx: 0, dy: 0, stickBits: 0 };
  const sticks = new Map();
  const looks = new Map();
  const pad = el("touch-pad");
  const stick = el("stick");
  const knob = el("stick-knob");
  const box = el("touch-buttons");
  const buttons = new Map();
  const directions = actions.forward | actions.back | actions.left | actions.right;

  function turnOn() {
    if (state.on) return;
    state.on = true;
    document.body.classList.add("touch");
  }
  // A touch screen with no mouse starts with the controls showing; any touch
  // shows them on a screen that has both.
  if (window.matchMedia && window.matchMedia("(pointer: coarse)").matches) turnOn();
  window.addEventListener(
    "pointerdown",
    (e) => {
      if (e.pointerType === "touch") turnOn();
    },
    { capture: true },
  );

  // The stick's bits from how far the thumb is from where it went down.
  function aim(ox, oy) {
    const reach = touch.stick_radius * touch.dead_zone;
    let bits = 0;
    if (oy < -reach) bits |= actions.forward;
    if (oy > reach) bits |= actions.back;
    if (ox < -reach) bits |= actions.left;
    if (ox > reach) bits |= actions.right;
    return bits;
  }

  // the knob stays within the stick's square
  const within = (v) => (v > touch.stick_radius ? touch.stick_radius : v < -touch.stick_radius ? -touch.stick_radius : v);
  function placeKnob(ox, oy) {
    knob.style.transform = `translate(${within(ox)}px, ${within(oy)}px)`;
  }

  function stickMove(p, e) {
    const ox = e.clientX - p.x;
    const oy = e.clientY - p.y;
    state.held &= ~directions;
    state.stickBits = aim(ox, oy);
    state.held |= state.stickBits;
    state.tapped |= state.stickBits;
    placeKnob(ox, oy);
  }

  function stickEnd() {
    state.held &= ~directions;
    state.stickBits = 0;
    stick.classList.remove("live");
    stick.style.left = "";
    stick.style.top = "";
    placeKnob(0, 0);
  }

  pad.addEventListener("pointerdown", (e) => {
    turnOn();
    e.preventDefault();
    capture(pad, e);
    if (e.clientX < window.innerWidth * touch.stick_side && sticks.size === 0) {
      sticks.set(e.pointerId, { x: e.clientX, y: e.clientY });
      stick.classList.add("live");
      stick.style.left = `${e.clientX}px`;
      stick.style.top = `${e.clientY}px`;
    } else {
      looks.set(e.pointerId, { x: e.clientX, y: e.clientY });
    }
  });
  pad.addEventListener("pointermove", (e) => {
    const s = sticks.get(e.pointerId);
    if (s) return stickMove(s, e);
    const l = looks.get(e.pointerId);
    if (!l) return;
    state.dx += (e.clientX - l.x) * touch.look_scale;
    state.dy += (e.clientY - l.y) * touch.look_scale;
    l.x = e.clientX;
    l.y = e.clientY;
  });
  const lift = (e) => {
    if (sticks.delete(e.pointerId)) stickEnd();
    looks.delete(e.pointerId);
  };
  pad.addEventListener("pointerup", lift);
  pad.addEventListener("pointercancel", lift);

  function button(id, action) {
    const b = document.createElement("button");
    b.className = "touch-btn";
    b.dataset.id = id;
    b.dataset.action = action;
    const label = document.createElement("span");
    label.textContent = copy[`touch_${id}`];
    b.appendChild(label);
    const bit = actions[action];
    const down = (e) => {
      e.preventDefault();
      turnOn();
      capture(b, e);
      state.held |= bit;
      state.tapped |= bit;
    };
    const up = () => {
      state.held &= ~bit;
    };
    b.addEventListener("pointerdown", down);
    b.addEventListener("pointerup", up);
    b.addEventListener("pointercancel", up);
    b.addEventListener("contextmenu", (e) => e.preventDefault());
    box.appendChild(b);
    buttons.set(id, b);
    return b;
  }

  for (const spec of touch.buttons) button(spec.id, spec.action).dataset.when = spec.when;

  function release() {
    state.held = 0;
    state.tapped = 0;
    sticks.clear();
    looks.clear();
    stickEnd();
  }

  return {
    on: () => state.on,
    turnOn,
    release,
    // The held bits, and the look since the last read.
    read() {
      const out = { bits: state.held | state.tapped, dx: state.dx, dy: state.dy };
      state.tapped = 0;
      state.dx = 0;
      state.dy = 0;
      return out;
    },
    // Which buttons show, from what the core says: the car's own while you
    // drive, and a button for each ability it has, filled as it charges.
    update(h) {
      document.body.dataset.where = h.driving ? "car" : "foot";
      const have = new Set(h.driving ? h.abilities.map((a) => a.id) : []);
      for (const a of h.driving ? h.abilities : []) {
        const b = buttons.get(a.id) || button(a.id, touch.abilities[a.id]);
        b.dataset.when = "car";
        b.dataset.state = a.state;
        b.style.setProperty("--level", a.level);
      }
      for (const [id, b] of buttons) {
        if (touch.abilities[id] && !have.has(id)) {
          b.remove();
          buttons.delete(id);
        }
      }
    },
    // Lights the buttons and moves the stick for the bits given: the ones held
    // now, or, when the game is filmed, the ones the film presses.
    show(bits, film) {
      for (const b of buttons.values()) b.classList.toggle("down", Boolean(bits & actions[b.dataset.action]));
      if (!film) return;
      const r = touch.stick_radius;
      const ox = (Boolean(bits & actions.right) - Boolean(bits & actions.left)) * r;
      const oy = (Boolean(bits & actions.back) - Boolean(bits & actions.forward)) * r;
      stick.classList.toggle("live", Boolean(bits & directions));
      placeKnob(ox, oy);
    },
  };
}
