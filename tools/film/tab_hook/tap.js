// For tools/film/record_tab.py: everything the page plays through Web Audio
// is copied to a MediaStreamDestination (window.__tap) the recorder can
// record, and media elements are routed through the same context once a
// recording starts. Loaded only into the recording window.
(() => {
  const connect = AudioNode.prototype.connect;
  window.__tap = null;
  AudioNode.prototype.connect = function (dest, ...rest) {
    const r = connect.call(this, dest, ...rest);
    if (dest instanceof AudioDestinationNode) {
      const ctx = dest.context;
      if (!ctx.__tap) {
        ctx.__tap = ctx.createMediaStreamDestination();
        window.__tap = ctx.__tap;
        window.__tapCtx = ctx;
        window.__bridge();
      }
      connect.call(this, ctx.__tap, ...rest);
    }
    return r;
  };
  // the recorder's own context (record_tab.py) takes the tap's stream,
  // whenever both exist: the page makes its context at the first tap
  window.__bridge = () => {
    if (!window.__tap || !window.__recDest || window.__bridged === window.__recDest) return;
    window.__bridged = window.__recDest;
    window.__recCtx.createMediaStreamSource(window.__tap.stream).connect(window.__recDest);
    window.__routeMedia();
  };
  const media = new Set();
  window.__routeMedia = () => {
    const ctx = window.__tapCtx;
    if (!ctx) return 0;
    for (const el of media) {
      if (el.__routed) continue;
      el.__routed = true;
      ctx.createMediaElementSource(el).connect(ctx.destination);
    }
    return media.size;
  };
  const play = HTMLMediaElement.prototype.play;
  HTMLMediaElement.prototype.play = function () {
    media.add(this);
    if (window.__rec && window.__rec.state === "recording") window.__routeMedia();
    return play.call(this);
  };
})();
