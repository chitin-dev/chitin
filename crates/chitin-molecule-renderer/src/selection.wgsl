@group(0) @binding(0) var mask: texture_2d<f32>;
@group(0) @binding(1) var<uniform> width: vec4<f32>;

@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
  let x = f32((index << 1u) & 2u);
  let y = f32(index & 2u);
  return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

// Outline inward only: occluders write zero and cannot acquire a halo.
@fragment fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
  let point = vec2<i32>(position.xy);
  let limit = vec2<i32>(textureDimensions(mask)) - vec2<i32>(1);
  if textureLoad(mask, point, 0).r < 0.5 { discard; }
  let radius = max(1, i32(ceil(width.x)));
  var edge = false;
  for (var y = -radius; y <= radius; y += 1) {
    for (var x = -radius; x <= radius; x += 1) {
      if f32(x*x + y*y) > width.x * width.x { continue; }
      let neighbor = point + vec2<i32>(x, y);
      if any(neighbor < vec2<i32>(0)) || any(neighbor > limit) {
        edge = true;
      } else if textureLoad(mask, neighbor, 0).r < 0.5 {
        edge = true;
      }
    }
  }
  if !edge { discard; }
  return vec4<f32>(0.18, 0.9, 0.38, 1.0);
}
