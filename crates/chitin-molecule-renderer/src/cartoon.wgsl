// Cartoon vertices use the same camera, lighting, depth-cue, and background
// uniform layout as analytic atom and bond surfaces. Rust writes this block
// once per frame before the render pass begins.
struct Uniforms {
  mvp: mat4x4<f32>,
  model_view: mat4x4<f32>,
  projection: mat4x4<f32>,
  key_light: vec4<f32>,
  fill_light: vec4<f32>,
  material: vec4<f32>,
  depth_cue: vec4<f32>,
  background: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct CartoonVertex {
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) color: vec3<f32>,
}

// The vertex stage applies the fitted model-view-projection transform while
// keeping the source-space color and surface normal available to the fragment
// stage for continuous lighting across each ribbon.
struct VertexOutput {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) view_position: vec3<f32>,
  @location(1) view_normal: vec3<f32>,
  @location(2) color: vec3<f32>,
}

@vertex
fn cartoon_vertex(vertex: CartoonVertex) -> VertexOutput {
  var out: VertexOutput;
  let view_position = uniforms.model_view * vec4<f32>(vertex.position, 1.0);
  out.clip_position = uniforms.projection * view_position;
  out.view_position = view_position.xyz;
  out.view_normal = normalize((uniforms.model_view * vec4<f32>(vertex.normal, 0.0)).xyz);
  out.color = vertex.color;
  return out;
}

// Wrapped diffuse lighting keeps the relatively flat cartoon sections legible
// when their normals turn away from either directional light.
fn wrapped_diffuse(normal: vec3<f32>, light_direction: vec3<f32>, strength: f32) -> f32 {
  let wrap = 0.15;
  let facing = max(-dot(normal, light_direction), 0.0);
  return ((facing + wrap) / (1.0 + wrap)) * strength;
}

@fragment
fn cartoon_fragment(in: VertexOutput) -> @location(0) vec4<f32> {
  let normal = normalize(in.view_normal);
  let key_direction = normalize(uniforms.key_light.xyz);
  let fill_direction = normalize(uniforms.fill_light.xyz);
  let key_diffuse = wrapped_diffuse(normal, key_direction, uniforms.key_light.w);
  let fill_diffuse = wrapped_diffuse(normal, fill_direction, uniforms.fill_light.w);
  let view_direction = normalize(-in.view_position);
  let reflected_key = normalize(reflect(key_direction, normal));
  let specular = pow(max(dot(reflected_key, view_direction), 0.0), uniforms.material.z) * uniforms.material.y;
  let diffuse = uniforms.material.x + (key_diffuse + fill_diffuse) * uniforms.material.w;
  let lit_color = in.color * diffuse + vec3<f32>(specular);
  let linear_depth = -in.view_position.z;
  let cue = smoothstep(uniforms.depth_cue.x, uniforms.depth_cue.y, linear_depth) * uniforms.depth_cue.z;
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
  return vec4<f32>(mix(lit_color, uniforms.background.rgb, cue), 1.0);
}
