struct Uniforms {
  mvp: mat4x4<f32>,
  bounds_min: vec4<f32>,
  bounds_max: vec4<f32>,
  slice: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var scalar_volume: texture_3d<f32>;

struct VertexInput {
  @location(0) uv: vec2<f32>,
};

struct VertexOutput {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
  var output: VertexOutput;
  let position = mix(
    uniforms.bounds_min.xyz,
    uniforms.bounds_max.xyz,
    vec3<f32>(input.uv, uniforms.slice.x),
  );
  output.clip_position = uniforms.mvp * vec4<f32>(position, 1.0);
  output.uv = input.uv;
  return output;
}

fn tetrahedral_value(grid_coordinate: vec3<f32>, dimensions: vec3<i32>) -> f32 {
  let maximum_base = dimensions - vec3<i32>(2);
  let base = clamp(vec3<i32>(floor(grid_coordinate)), vec3<i32>(0), maximum_base);
  let fraction = clamp(grid_coordinate - vec3<f32>(base), vec3<f32>(0.0), vec3<f32>(1.0));
  let v000 = textureLoad(scalar_volume, base, 0).r;
  let v100 = textureLoad(scalar_volume, base + vec3<i32>(1, 0, 0), 0).r;
  let v110 = textureLoad(scalar_volume, base + vec3<i32>(1, 1, 0), 0).r;
  let v010 = textureLoad(scalar_volume, base + vec3<i32>(0, 1, 0), 0).r;
  let v001 = textureLoad(scalar_volume, base + vec3<i32>(0, 0, 1), 0).r;
  let v101 = textureLoad(scalar_volume, base + vec3<i32>(1, 0, 1), 0).r;
  let v111 = textureLoad(scalar_volume, base + vec3<i32>(1, 1, 1), 0).r;
  let v011 = textureLoad(scalar_volume, base + vec3<i32>(0, 1, 1), 0).r;

  // The branch selects one simplex from the same globally compatible
  // Freudenthal decomposition used to extract the probe-center candidates.
  if fraction.x >= fraction.y {
    if fraction.y >= fraction.z {
      return v000
        + fraction.x * (v100 - v000)
        + fraction.y * (v110 - v100)
        + fraction.z * (v111 - v110);
    }
    if fraction.x >= fraction.z {
      return v000
        + fraction.x * (v100 - v000)
        + fraction.z * (v101 - v100)
        + fraction.y * (v111 - v101);
    }
    return v000
      + fraction.z * (v001 - v000)
      + fraction.x * (v101 - v001)
      + fraction.y * (v111 - v101);
  }
  if fraction.x >= fraction.z {
    return v000
      + fraction.y * (v010 - v000)
      + fraction.x * (v110 - v010)
      + fraction.z * (v111 - v110);
  }
  if fraction.y >= fraction.z {
    return v000
      + fraction.y * (v010 - v000)
      + fraction.z * (v011 - v010)
      + fraction.x * (v111 - v011);
  }
  return v000
    + fraction.z * (v001 - v000)
    + fraction.y * (v011 - v001)
    + fraction.x * (v111 - v011);
}

fn scalar_color(value: f32) -> vec4<f32> {
  let zero_band = 0.11;
  if abs(value) <= zero_band {
    return vec4<f32>(1.0, 0.86, 0.18, 0.92);
  }
  let magnitude = clamp(abs(value) / 2.0, 0.0, 1.0);
  if value < 0.0 {
    return vec4<f32>(0.08, 0.34 + 0.24 * (1.0 - magnitude), 1.0, 0.58);
  }
  return vec4<f32>(1.0, 0.16 + 0.24 * (1.0 - magnitude), 0.08, 0.20 + 0.24 * magnitude);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
  let dimensions = vec3<i32>(uniforms.slice.yzw);
  let grid_coordinate = vec3<f32>(
    input.uv.x * f32(dimensions.x - 1),
    input.uv.y * f32(dimensions.y - 1),
    clamp(uniforms.slice.x, 0.0, 1.0) * f32(dimensions.z - 1),
  );
  let value = tetrahedral_value(grid_coordinate, dimensions);
  var color = scalar_color(value);

  // Draw every actual X/Y sample line. Derivative-based widths keep the
  // lattice readable when the molecular-space plane is tilted or zoomed.
  let grid_fraction = fract(grid_coordinate.xy);
  let grid_distance = min(grid_fraction, vec2<f32>(1.0) - grid_fraction);
  let line_width = max(fwidth(grid_coordinate.xy) * 0.65, vec2<f32>(0.018));
  let grid_alpha = max(
    1.0 - smoothstep(0.0, line_width.x, grid_distance.x),
    1.0 - smoothstep(0.0, line_width.y, grid_distance.y),
  );
  let grid_color = mix(color.rgb, vec3<f32>(0.76, 0.84, 0.96), 0.34 * grid_alpha);
  let grid_opacity = max(color.a, 0.38 * grid_alpha);
  return vec4<f32>(grid_color, grid_opacity);
}

