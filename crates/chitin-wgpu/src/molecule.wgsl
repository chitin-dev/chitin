struct Uniforms {
  mvp: mat4x4<f32>,
  model_view: mat4x4<f32>,
  key_light: vec4<f32>,
  fill_light: vec4<f32>,
  material: vec4<f32>,
  depth_cue: vec4<f32>,
  background: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct MeshVertex {
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
}

struct AtomInstance {
  @location(2) position_radius: vec4<f32>,
  @location(3) color: vec4<f32>,
}

struct BondInstance {
  @location(2) start_radius: vec4<f32>,
  @location(3) end_padding: vec4<f32>,
  @location(4) color: vec4<f32>,
}

struct VertexOutput {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) color: vec3<f32>,
  @location(2) view_position: vec3<f32>,
}

fn camera_normal(scene_normal: vec3<f32>) -> vec3<f32> {
  let normal_matrix = mat3x3<f32>(
    uniforms.model_view[0].xyz,
    uniforms.model_view[1].xyz,
    uniforms.model_view[2].xyz,
  );
  return normalize(normal_matrix * scene_normal);
}

// Keeps camera-relative directional lighting readable on back-facing molecular
// surfaces without turning it into an unlit flat-color representation.
fn wrapped_diffuse(normal: vec3<f32>, light_direction: vec3<f32>, strength: f32) -> f32 {
  let wrap = 0.35;
  let facing = max(-dot(normal, light_direction), 0.0);
  return ((facing + wrap) / (1.0 + wrap)) * strength;
}

@vertex
fn atom_vertex(vertex: MeshVertex, instance: AtomInstance) -> VertexOutput {
  var out: VertexOutput;
  let world_position = instance.position_radius.xyz + vertex.position * instance.position_radius.w;
  let view_position = uniforms.model_view * vec4<f32>(world_position, 1.0);
  out.clip_position = uniforms.mvp * vec4<f32>(world_position, 1.0);
  out.normal = camera_normal(vertex.normal);
  out.color = instance.color.rgb;
  out.view_position = view_position.xyz;
  return out;
}

@vertex
fn bond_vertex(vertex: MeshVertex, instance: BondInstance) -> VertexOutput {
  var out: VertexOutput;
  let start = instance.start_radius.xyz;
  let end = instance.end_padding.xyz;
  let direction = end - start;
  let bond_length = max(length(direction), 0.0001);
  let up = direction / bond_length;

  // Select a helper that is not parallel to the bond direction, then build an
  // orthonormal frame around the local cylinder's Y axis.
  var helper = vec3<f32>(0.0, 1.0, 0.0);
  if abs(up.y) > 0.95 {
    helper = vec3<f32>(1.0, 0.0, 0.0);
  }
  let right = normalize(cross(helper, up));
  let forward = cross(up, right);
  let center = (start + end) * 0.5;
  let radial = right * vertex.position.x + forward * vertex.position.z;
  let world_position = center
    + radial * instance.start_radius.w
    + up * vertex.position.y * bond_length;
  let scene_normal = normalize(right * vertex.normal.x + forward * vertex.normal.z);
  let view_position = uniforms.model_view * vec4<f32>(world_position, 1.0);

  out.clip_position = uniforms.mvp * vec4<f32>(world_position, 1.0);
  out.normal = camera_normal(scene_normal);
  out.color = instance.color.rgb;
  out.view_position = view_position.xyz;
  return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
  let normal = normalize(in.normal);
  let key_direction = normalize(uniforms.key_light.xyz);
  let fill_direction = normalize(uniforms.fill_light.xyz);
  // Light directions describe the direction rays travel, matching ChimeraX's
  // camera-space convention, so the surface-to-light vector is their negation.
  let key_diffuse = wrapped_diffuse(normal, key_direction, uniforms.key_light.w);
  let fill_diffuse = wrapped_diffuse(normal, fill_direction, uniforms.fill_light.w);

  let view_direction = normalize(-in.view_position);
  let reflected_key = normalize(reflect(key_direction, normal));
  let specular = pow(max(dot(reflected_key, view_direction), 0.0), uniforms.material.z)
    * uniforms.material.y;
  let diffuse = uniforms.material.x
    + (key_diffuse + fill_diffuse) * uniforms.material.w;
  let lit_color = in.color * diffuse + vec3<f32>(specular);
  let linear_depth = -in.view_position.z;
  let cue = smoothstep(uniforms.depth_cue.x, uniforms.depth_cue.y, linear_depth)
    * uniforms.depth_cue.z;
  let debug_mode = u32(uniforms.depth_cue.w + 0.5);
  if debug_mode == 1u {
    return vec4<f32>(normal * 0.5 + vec3<f32>(0.5), 1.0);
  }
  if debug_mode == 2u {
    return vec4<f32>(vec3<f32>(key_diffuse), 1.0);
  }
  if debug_mode == 3u {
    return vec4<f32>(vec3<f32>(fill_diffuse), 1.0);
  }
  if debug_mode == 4u {
    return vec4<f32>(vec3<f32>(specular), 1.0);
  }
  if debug_mode == 5u {
    return vec4<f32>(vec3<f32>(cue), 1.0);
  }
  if debug_mode == 6u {
    return vec4<f32>(in.color, 1.0);
  }
  let color = mix(lit_color, uniforms.background.rgb, cue);
  return vec4<f32>(color, 1.0);
}
