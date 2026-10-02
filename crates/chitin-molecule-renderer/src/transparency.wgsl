@group(0) @binding(0) var accumulation: texture_2d<f32>;
@group(0) @binding(1) var revealage: texture_2d<f32>;

@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
  let x = f32((index << 1u) & 2u);
  let y = f32(index & 2u);
  return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
  let p = vec2<i32>(position.xy);
  let value = textureLoad(accumulation, p, 0);
  let reveal = clamp(textureLoad(revealage, p, 0).r, 0.0, 1.0);
  return vec4<f32>(value.rgb / max(value.a, 0.00001), 1.0 - reveal);
}
