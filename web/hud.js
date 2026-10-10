// The heads-up display: it writes what the core's hud() says into the page.
// Every word comes from data/copy.json; every number arrives as text.

const el = (id) => document.getElementById(id);

export function makeHud(copy) {
  const fill = (template, item) => template.replace("{item}", item || "");
  const abilities = el("abilities");
  const rows = new Map();
  const mirrors = el("mirrors");
  const frames = [];
  let songSeq = null;

  function ability(a) {
    let row = rows.get(a.id);
    if (!row) {
      row = document.createElement("div");
      row.className = "ability";
      const key = document.createElement("span");
      key.className = "key";
      key.textContent = copy[`key_${a.id}`];
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = copy[`ability_${a.id}`];
      const bar = document.createElement("span");
      bar.className = "bar";
      const level = document.createElement("i");
      bar.appendChild(level);
      row.append(key, name, bar);
      abilities.appendChild(row);
      rows.set(a.id, { row, level });
      row = rows.get(a.id);
    }
    row.row.dataset.state = a.state;
    row.level.style.width = a.level;
  }

  function placeMirrors(list) {
    while (frames.length < list.length) {
      const f = document.createElement("div");
      f.className = "mirror";
      mirrors.appendChild(f);
      frames.push(f);
    }
    frames.forEach((f, i) => {
      const m = list[i];
      f.hidden = !m;
      if (!m) return;
      [f.style.left, f.style.top, f.style.width, f.style.height] = m;
    });
  }

  return {
    update(h) {
      const chase = h.mode === "chase" || h.mode === "paused" && h.monster;
      el("reticle").hidden = h.driving;
      el("dread").style.opacity = h.dread;
      el("danger").style.opacity = h.danger;
      const prompt = el("prompt");
      const words = h.prompt ? fill(copy[`prompt_${h.prompt}`], h.held || h.aim) : "";
      prompt.hidden = !words;
      prompt.textContent = words;
      prompt.dataset.kind = h.prompt;
      for (const id of ["chase-top", "chase-left"]) el(id).hidden = !chase;
      el("chase-right").hidden = !(chase && h.driving);
      el("h-time").textContent = h.time;
      el("h-best").textContent = h.best;
      el("h-monster").textContent = h.monster || "";
      el("h-distance").textContent = h.distance;
      el("h-speed").textContent = h.speed;
      for (const a of h.abilities) ability(a);
      // a new chase draws its abilities afresh: drop the rows it lacks
      for (const [id, r] of rows) {
        if (!h.abilities.some((a) => a.id === id)) {
          r.row.remove();
          rows.delete(id);
        }
      }
      el("h-next").textContent = h.next ? `${copy.next_label} ${h.next.in}` : "";
      placeMirrors(h.mirrors);
      const marker = el("marker");
      marker.hidden = !h.car_marker;
      if (h.car_marker) [marker.style.left, marker.style.top] = h.car_marker;
      // the record player's callout: the song's name, popped afresh on a change
      const callout = el("record-callout");
      callout.hidden = !h.player_marker;
      if (h.player_marker) [callout.style.left, callout.style.top] = h.player_marker;
      const word = el("record-song");
      word.textContent = h.song;
      if (h.song_seq !== songSeq) {
        songSeq = h.song_seq;
        word.classList.remove("pop");
        void word.offsetWidth;
        word.classList.add("pop");
      }
      const toast = el("toast");
      toast.hidden = !h.toast;
      if (h.toast) {
        el("t-name").textContent = h.toast.name;
        el("t-text").textContent = h.toast.text;
      }
      el("flipnote").hidden = !h.upside_down;
    },
  };
}
