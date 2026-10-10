#version 300 es
// One shape: the mesh vertex placed by the instance's three scaled axes and
// translation (iA, iB, iC; translation in .w), coloured by iD (rgb, and the
// style in .a). Every value arrives as the core's Q16 integer.
in vec3 aPos;
in vec3 aNormal;
in vec4 iA;
in vec4 iB;
in vec4 iC;
in vec4 iD;
uniform mat4 uVP;
uniform float uPass;
// the ink line: least width, growth per metre of the shape, most width
uniform vec3 uInkSize;
out vec3 vWorld;
out vec3 vNormal;
out vec4 vColor;
// position and normal in the shape's own unit box, and its size in metres
out vec3 vLocal;
out vec3 vLocalNormal;
out vec3 vSize;
out vec2 vUV;
const float Q = 1.0 / 65536.0;
void main() {
  vec3 a = iA.xyz * Q;
  vec3 b = iB.xyz * Q;
  vec3 c = iC.xyz * Q;
  vec3 t = vec3(iA.w, iB.w, iC.w) * Q;
  vec3 p = aPos * Q;
  vec3 n = aNormal * Q;
  vec3 size = vec3(length(a), length(b), length(c));
  vLocal = p;
  if (uPass > 1.5) {
    // the inverted hull: each axis grown by the line's width on both sides
    float big = max(size.x, max(size.y, size.z));
    float w = clamp(uInkSize.x + big * uInkSize.y, uInkSize.x, uInkSize.z);
    p *= (size + 2.0 * w) / max(size, vec3(1e-4));
  }
  vec3 world = a * p.x + b * p.y + c * p.z + t;
  vNormal = a * (n.x / max(dot(a, a), 1e-8)) + b * (n.y / max(dot(b, b), 1e-8)) + c * (n.z / max(dot(c, c), 1e-8));
  vLocalNormal = n;
  vSize = size;
  vColor = iD * Q;
  vUV = vec2(aPos.x * Q + 0.5, 0.5 - aPos.y * Q);
  vWorld = world;
  gl_Position = uVP * vec4(world, 1.0);
}
