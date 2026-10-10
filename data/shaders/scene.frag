#version 300 es
precision highp float;
// Three passes (the core says which): 0 solid, lit in flat bands like the
// TikZ art's fills, with windows drawn on the sides of buildings; 1 the
// pursuer's picture from its texture, cardboard on the back; 2 ink, the
// plum outline.
in vec3 vWorld;
in vec3 vNormal;
in vec4 vColor;
in vec3 vLocal;
in vec3 vLocalNormal;
in vec3 vSize;
in vec2 vUV;
uniform vec3 uEye;
uniform vec3 uLightDir;
uniform vec3 uSky;
uniform vec3 uFog;
uniform vec2 uFogRange;
uniform vec3 uLight;
uniform vec3 uWindowDark;
uniform vec3 uWindowLit;
uniform vec3 uInk;
uniform float uPass;
uniform sampler2D uTex;
out vec4 outColor;
// styles (crates/core/src/city.rs): 1 tower, 2 mid-rise, 3 house, 4 deck, 5 glow
// a window grid for each building style: cell width and height in metres,
// the pane's half size and its frame's, as fractions of the cell
const vec2 TOWER_CELL = vec2(4.0, 4.0);
const vec2 MID_CELL = vec2(3.0, 3.5);
const vec2 HOUSE_CELL = vec2(4.0, 3.0);
const vec3 CARDBOARD = vec3(0.72, 0.51, 0.35);
const float LIT = 0.42;

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
  float d = length(vWorld - uEye);
  float f = clamp((d - uFogRange.x) / (uFogRange.y - uFogRange.x), 0.0, 1.0);
  f = f * f * (3.0 - 2.0 * f);
  if (uPass > 1.5) {
    outColor = vec4(mix(uInk, uFog, f * 0.85), 1.0);
    return;
  }
  if (uPass > 0.5) {
    vec4 tex = texture(uTex, vUV);
    if (tex.a < 0.5) discard;
    vec3 c = gl_FrontFacing ? tex.rgb : CARDBOARD * (0.8 + 0.2 * tex.r);
    outColor = vec4(mix(c, uFog, f * 0.55), 1.0);
    return;
  }
  vec3 n = normalize(vNormal);
  float style = floor(vColor.a + 0.001);
  float seed = fract(vColor.a);
  vec3 base = vColor.rgb;
  vec3 ln = vLocalNormal;
  bool side = abs(ln.y) < 0.5;
  if (style > 0.5 && style < 3.5 && side) {
    vec2 cell = style < 1.5 ? TOWER_CELL : (style < 2.5 ? MID_CELL : HOUSE_CELL);
    float u = abs(ln.x) > abs(ln.z) ? (vLocal.z + 0.5) * vSize.z : (vLocal.x + 0.5) * vSize.x;
    float v = (vLocal.y + 0.5) * vSize.y;
    vec2 g = vec2(u, v) / cell;
    vec2 at = abs(fract(g) - 0.5);
    vec2 pane = style > 2.5 ? vec2(0.18, 0.2) : vec2(0.3, 0.3);
    if (at.x < pane.x + 0.05 && at.y < pane.y + 0.05) {
      bool lit = hash(floor(g) + seed * 113.0 + ln.xz * 7.0) < LIT;
      vec3 w = lit ? uWindowLit : uWindowDark;
      if (at.x > pane.x || at.y > pane.y) w = uInk;
      base = w;
    }
  }
  if (style > 3.5 && style < 4.5 && ln.y > 0.5) {
    // the deck's paint: a dashed line down each lane's middle
    if (abs(vLocal.z) < 0.02 && fract((vLocal.x + 0.5) * vSize.x / 6.0) < 0.5) base = uWindowLit;
  }
  if (!side && ln.y > 0.5 && style > 0.5 && style < 3.5) base *= 0.82;
  if (style > 4.5) {
    outColor = vec4(base * 1.3, 1.0);
    return;
  }
  float diffuse = dot(n, uLightDir);
  float band = diffuse > 0.55 ? 1.0 : (diffuse > 0.12 ? 0.74 : 0.52);
  float sky = 0.5 + 0.5 * n.y;
  vec3 ambient = mix(uFog * 0.55, uSky * 0.8, sky);
  vec3 lit = base * (ambient * 0.72 + uLight * band * 0.52);
  outColor = vec4(mix(lit, uFog, f), 1.0);
}
