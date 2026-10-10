// The WebGL 2 renderer. It uploads what the core returns and draws it; every
// size, stride and offset comes from the core's numbers(), so this file holds
// no layout of its own. Instances and meshes are the core's Q16 integers,
// read as integer attributes and scaled in the shader.
//
// The city's pieces live in one buffer uploaded whole once and then only in
// the runs the core marks; the scenery is uploaded once; the rest each frame.
// The core lists the views (the windscreen and the mirrors) and each view's
// draws; this file sets the viewport and the GL state each draw asks for.

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader));
  }
  return shader;
}

function link(gl, vertexSource, fragmentSource) {
  const program = gl.createProgram();
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vertexSource));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fragmentSource));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program));
  }
  return program;
}

// Returns null when the browser has no WebGL 2; the page then says so.
export function makeRenderer(canvas, N, shaders, meshData, images) {
  const gl = canvas.getContext("webgl2", { antialias: true, powerPreference: "high-performance" });
  if (!gl) return null;

  const info = gl.getExtension("WEBGL_debug_renderer_info");
  const name = info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : "";
  const software = shaders.software.some((s) => name.includes(s));

  const program = link(gl, shaders.vertex, shaders.fragment);
  gl.useProgram(program);
  gl.enable(gl.DEPTH_TEST);
  gl.enable(gl.CULL_FACE);
  gl.enable(gl.SCISSOR_TEST);

  const vao = gl.createVertexArray();
  gl.bindVertexArray(vao);

  const meshes = [];
  for (let id = 0; id < N.mesh_count; id++) {
    const data = meshData(id);
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    meshes.push({ buffer, vertices: data.length / N.vertex_values });
  }

  const textures = images.map((img) => {
    const t = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return t;
  });

  const attribute = ([attr, size, offset]) => ({ loc: gl.getAttribLocation(program, attr), size, offset });
  const vertexAttributes = N.vertex.attributes.map(attribute);
  const instanceAttributes = N.instance.attributes.map(attribute);
  for (const a of vertexAttributes.concat(instanceAttributes)) gl.enableVertexAttribArray(a.loc);
  for (const a of instanceAttributes) gl.vertexAttribDivisor(a.loc, 1);

  const uniform = (u) => gl.getUniformLocation(program, u);
  const sceneUniforms = N.uniforms.map(([u, size]) => ({ loc: uniform(u), size }));
  const uVP = uniform("uVP");
  const uEye = uniform("uEye");
  const uPass = uniform("uPass");
  gl.uniform1i(uniform("uTex"), 0);

  const buffers = [gl.createBuffer(), gl.createBuffer(), gl.createBuffer()];
  const P = N.passes;
  let clear = [0, 0, 0];

  function enterPass(pass) {
    gl.uniform1f(uPass, pass);
    if (pass === P.sprite) {
      gl.disable(gl.CULL_FACE);
      return;
    }
    gl.enable(gl.CULL_FACE);
    gl.cullFace(pass === P.ink ? gl.FRONT : gl.BACK);
  }

  return {
    software,
    // The whole city's slots, after a new chase or on the first frame.
    setPieces(values) {
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[N.buffers.pieces]);
      gl.bufferData(gl.ARRAY_BUFFER, values, gl.DYNAMIC_DRAW);
    },
    // Only the runs that moved: (byte offset, value offset, value count).
    updatePieces(values, dirty) {
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[N.buffers.pieces]);
      for (let i = 0; i < dirty.length; i += N.dirty_values) {
        const [byteOffset, from, count] = dirty.subarray(i, i + N.dirty_values);
        gl.bufferSubData(gl.ARRAY_BUFFER, byteOffset, values, from, count);
      }
    },
    setScene(values, uniformValues, clearColor) {
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[N.buffers.scenery]);
      gl.bufferData(gl.ARRAY_BUFFER, values, gl.STATIC_DRAW);
      let at = 0;
      for (const u of sceneUniforms) {
        gl[`uniform${u.size}fv`](u.loc, uniformValues.subarray(at, at + u.size));
        at += u.size;
      }
      clear = clearColor;
    },
    resize(width, height) {
      canvas.width = width;
      canvas.height = height;
    },
    frame(rects, mats, draws, dynamic) {
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[N.buffers.dynamic]);
      gl.bufferData(gl.ARRAY_BUFFER, dynamic, gl.STREAM_DRAW);
      let view = -1;
      let pass = -1;
      for (let i = 0; i < draws.length; i += N.draw_values) {
        const [v, buffer, meshId, byteOffset, count, drawPass, texture] = draws.subarray(i, i + N.draw_values);
        if (v !== view) {
          view = v;
          const [x, y, w, h, mirror] = rects.subarray(v * N.rect_values, v * N.rect_values + N.rect_values);
          gl.viewport(x, y, w, h);
          gl.scissor(x, y, w, h);
          const [r, g, b] = clear;
          gl.clearColor(r, g, b, 1);
          gl.depthMask(true);
          gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
          const m = mats.subarray(v * N.mat_values, v * N.mat_values + N.mat_values);
          gl.uniformMatrix4fv(uVP, false, m.subarray(0, N.view_values));
          gl.uniform3fv(uEye, m.subarray(N.view_values));
          gl.frontFace(mirror ? gl.CW : gl.CCW);
          pass = -1;
        }
        if (drawPass !== pass) {
          enterPass(drawPass);
          pass = drawPass;
        }
        if (drawPass === P.sprite) {
          gl.activeTexture(gl.TEXTURE0);
          gl.bindTexture(gl.TEXTURE_2D, textures[texture]);
        }
        const mesh = meshes[meshId];
        gl.bindBuffer(gl.ARRAY_BUFFER, mesh.buffer);
        for (const a of vertexAttributes) {
          gl.vertexAttribPointer(a.loc, a.size, gl.INT, false, N.vertex.stride, a.offset);
        }
        gl.bindBuffer(gl.ARRAY_BUFFER, buffers[buffer]);
        for (const a of instanceAttributes) {
          gl.vertexAttribPointer(a.loc, a.size, gl.INT, false, N.instance.stride, a.offset + byteOffset);
        }
        gl.drawArraysInstanced(gl.TRIANGLES, 0, mesh.vertices, count);
      }
      gl.frontFace(gl.CCW);
    },
  };
}
