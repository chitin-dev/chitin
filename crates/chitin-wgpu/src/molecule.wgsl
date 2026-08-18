struct Uniforms {
  mvp: mat4x4<f32>,
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
}

@vertex
fn atom_vertex(vertex: MeshVertex, instance: AtomInstance) -> VertexOutput {
  var out: VertexOutput;
  let world_position = instance.position_radius.xyz + vertex.position * instance.position_radius.w;
  out.clip_position = uniforms.mvp * vec4<f32>(world_position, 1.0);
  out.normal = vertex.normal;
  out.color = instance.color.rgb;
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

  out.clip_position = uniforms.mvp * vec4<f32>(world_position, 1.0);
  out.normal = normalize(right * vertex.normal.x + forward * vertex.normal.z);
  out.color = instance.color.rgb;
  return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
  let normal = normalize(in.normal);
  let key_light = max(dot(normal, normalize(vec3<f32>(0.45, 0.75, 0.55))), 0.0);
  let fill_light = max(dot(normal, normalize(vec3<f32>(-0.6, 0.2, -0.4))), 0.0);
  let light = 0.24 + key_light * 0.68 + fill_light * 0.16;
  return vec4<f32>(in.color * light, 1.0);
}
