// The CPU writes this block as three matrices followed by lighting, material,
// depth-cue, and background parameters. Keep the field order synchronized with
// `material_uniform` and the byte offsets used by the Rust renderer.
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

// The shared mesh is a unit quad. Its position.xy values become billboard
// coordinates; analytic sphere/cylinder intersections provide the real shape.
struct MeshVertex {
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
}

// Atom instance data: xyz is the source-space center, w is the source radius.
struct AtomInstance {
  @location(2) position_radius: vec4<f32>,
  @location(3) color: vec4<f32>,
}

// Bond instance data: start_radius.xyz/end_padding.xyz are endpoints and
// start_radius.w is the cylinder radius. The color vectors hold endpoint colors.
struct BondInstance {
  @location(2) start_radius: vec4<f32>,
  @location(3) end_padding: vec4<f32>,
  @location(4) start_color: vec4<f32>,
  @location(5) end_color: vec4<f32>,
}

// The vertex stage expands a quad around each primitive. The fragment stage
// uses these values to reconstruct a camera-space ray and solve the surface.
struct VertexOutput {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) ray_anchor: vec3<f32>,
  @location(1) surface_start_radius: vec4<f32>,
  @location(2) surface_end: vec3<f32>,
  @location(3) start_color: vec3<f32>,
  @location(4) end_color: vec3<f32>,
}

// Analytic surfaces write their own depth after finding the exact intersection.
struct FragmentOutput {
  @location(0) color: vec4<f32>,
  @builtin(frag_depth) depth: f32,
}

fn view_radius(source_radius: f32) -> f32 {
  // Only the linear part of model_view affects a radius; translation is zero.
  return length((uniforms.model_view * vec4<f32>(source_radius, 0.0, 0.0, 0.0)).xyz);
}

fn projected_radius(radius: f32, view_depth: f32) -> vec2<f32> {
  // Project the tangent of the surface instead of its center. The lower bound
  // avoids a divide-by-zero when the camera is close to or inside a primitive.
  let tangent_depth = sqrt(max(view_depth * view_depth - radius * radius, 0.000001));
  return vec2<f32>(uniforms.projection[0][0], uniforms.projection[1][1])
    * radius / tangent_depth;
}

fn view_ray_anchor(ndc: vec2<f32>, view_depth: f32) -> vec3<f32> {
  // Reconstruct a point on the camera ray at the supplied positive depth.
  return vec3<f32>(
    ndc.x * view_depth / uniforms.projection[0][0],
    ndc.y * view_depth / uniforms.projection[1][1],
    -view_depth,
  );
}

@vertex
fn atom_vertex(vertex: MeshVertex, instance: AtomInstance) -> VertexOutput {
  var out: VertexOutput;
  let center = (uniforms.model_view * vec4<f32>(instance.position_radius.xyz, 1.0)).xyz;
  let radius = view_radius(instance.position_radius.w);
  let center_clip = uniforms.projection * vec4<f32>(center, 1.0);
  let center_ndc = center_clip.xy / center_clip.w;
  // Expand the unit quad in NDC so every possible sphere intersection reaches
  // the fragment stage, including the silhouette.
  let view_depth = max(-center.z, radius + 0.0001);
  let ndc = center_ndc + vertex.position.xy * projected_radius(radius, view_depth) * 1.03;

  out.clip_position = vec4<f32>(ndc * center_clip.w, center_clip.z, center_clip.w);
  out.ray_anchor = view_ray_anchor(ndc, view_depth);
  out.surface_start_radius = vec4<f32>(center, radius);
  out.surface_end = center;
  out.start_color = instance.color.rgb;
  out.end_color = instance.color.rgb;
  return out;
}

@vertex
fn bond_vertex(vertex: MeshVertex, instance: BondInstance) -> VertexOutput {
  var out: VertexOutput;
  let start = (uniforms.model_view * vec4<f32>(instance.start_radius.xyz, 1.0)).xyz;
  let end = (uniforms.model_view * vec4<f32>(instance.end_padding.xyz, 1.0)).xyz;
  let radius = view_radius(instance.start_radius.w);
  let start_clip = uniforms.projection * vec4<f32>(start, 1.0);
  let end_clip = uniforms.projection * vec4<f32>(end, 1.0);
  let start_ndc = start_clip.xy / start_clip.w;
  let end_ndc = end_clip.xy / end_clip.w;
  // Project both endpoint radii and interpolate their extents along the bond.
  let start_extent = projected_radius(radius, max(-start.z, radius + 0.0001));
  let end_extent = projected_radius(radius, max(-end.z, radius + 0.0001));
  let unit_position = vertex.position.x * 0.5 + 0.5;
  let projected_axis = end_ndc - start_ndc;
  var projected_normal = vec2<f32>(1.0, 0.0);
  if dot(projected_axis, projected_axis) > 0.00000001 {
    projected_normal = normalize(vec2<f32>(-projected_axis.y, projected_axis.x));
  }
  let radial_extent = mix(start_extent, end_extent, unit_position);
  let normal_extent = length(projected_normal * radial_extent) * 1.02;
  let ndc = mix(start_ndc, end_ndc, unit_position)
    + projected_normal * vertex.position.y * normal_extent;
  let center = (start + end) * 0.5;
  let center_clip = uniforms.projection * vec4<f32>(center, 1.0);
  let view_depth = max(-center.z, radius + 0.0001);

  out.clip_position = vec4<f32>(ndc * center_clip.w, center_clip.z, center_clip.w);
  out.ray_anchor = view_ray_anchor(ndc, view_depth);
  out.surface_start_radius = vec4<f32>(start, radius);
  out.surface_end = end;
  out.start_color = instance.start_color.rgb;
  out.end_color = instance.end_color.rgb;
  return out;
}

fn wrapped_diffuse(normal: vec3<f32>, light_direction: vec3<f32>, strength: f32) -> f32 {
  // A small wrap term keeps surfaces readable when a normal is turned away
  // from a light, which is useful for dense molecular scenes.
  let wrap = 0.15;
  let facing = max(-dot(normal, light_direction), 0.0);
  return ((facing + wrap) / (1.0 + wrap)) * strength;
}

fn shade_surface(normal: vec3<f32>, view_position: vec3<f32>, element_color: vec3<f32>) -> vec4<f32> {
  let key_direction = normalize(uniforms.key_light.xyz);
  let fill_direction = normalize(uniforms.fill_light.xyz);
  let key_diffuse = wrapped_diffuse(normal, key_direction, uniforms.key_light.w);
  let fill_diffuse = wrapped_diffuse(normal, fill_direction, uniforms.fill_light.w);
  let view_direction = normalize(-view_position);
  let reflected_key = normalize(reflect(key_direction, normal));
  let specular = pow(max(dot(reflected_key, view_direction), 0.0), uniforms.material.z)
    * uniforms.material.y;
  let diffuse = uniforms.material.x
    + (key_diffuse + fill_diffuse) * uniforms.material.w;
  let silhouette = 0.90 + 0.10 * sqrt(abs(dot(normal, view_direction)));
  let lit_color = (element_color * diffuse + vec3<f32>(specular)) * silhouette;
  let linear_depth = -view_position.z;
  let cue = smoothstep(uniforms.depth_cue.x, uniforms.depth_cue.y, linear_depth)
    * uniforms.depth_cue.z;
  // Debug modes expose individual lighting/depth terms without changing the
  // geometry or depth written by the analytic surface.
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
    return vec4<f32>(element_color, 1.0);
  }
  return vec4<f32>(mix(lit_color, uniforms.background.rgb, cue), 1.0);
}

fn fragment_depth(view_position: vec3<f32>) -> f32 {
  // Convert the analytically reconstructed surface point into the same depth
  // range as the regular WGPU projection.
  let clip_position = uniforms.projection * vec4<f32>(view_position, 1.0);
  return clip_position.z / clip_position.w;
}

@fragment
fn atom_fragment(in: VertexOutput) -> FragmentOutput {
  let ray_direction = normalize(in.ray_anchor);
  let center = in.surface_start_radius.xyz;
  let radius = in.surface_start_radius.w;
  let center_along_ray = dot(ray_direction, center);
  // Solve the camera-ray/sphere quadratic in camera space.
  let discriminant = center_along_ray * center_along_ray - dot(center, center) + radius * radius;
  if discriminant < 0.0 {
    discard;
  }
  let distance = center_along_ray - sqrt(discriminant);
  // Only the nearest intersection in front of the camera is currently used.
  // A camera inside a sphere has no positive near intersection and is dropped.
  if distance <= 0.0 {
    discard;
  }
  let view_position = ray_direction * distance;
  let normal = normalize(view_position - center);
  var out: FragmentOutput;
  out.color = shade_surface(normal, view_position, in.start_color);
  out.depth = fragment_depth(view_position);
  return out;
}

@fragment
fn bond_fragment(in: VertexOutput) -> FragmentOutput {
  let ray_direction = normalize(in.ray_anchor);
  let start = in.surface_start_radius.xyz;
  let end = in.surface_end;
  let radius = in.surface_start_radius.w;
  let axis = end - start;
  let origin_to_start = -start;
  let axis_length_squared = dot(axis, axis);
  let axis_ray = dot(axis, ray_direction);
  let axis_origin = dot(axis, origin_to_start);
  let ray_origin = dot(ray_direction, origin_to_start);
  let origin_squared = dot(origin_to_start, origin_to_start);
  let quadratic_a = axis_length_squared - axis_ray * axis_ray;
  let quadratic_b = axis_length_squared * ray_origin - axis_origin * axis_ray;
  let quadratic_c = axis_length_squared * origin_squared
    - axis_origin * axis_origin
    - radius * radius * axis_length_squared;
  // Solve the ray/infinite-cylinder quadratic, then restrict the hit to the
  // finite segment between the two bond endpoints.
  let discriminant = quadratic_b * quadratic_b - quadratic_a * quadratic_c;
  if discriminant < 0.0 || quadratic_a < 0.000001 {
    discard;
  }
  let distance = (-quadratic_b - sqrt(discriminant)) / quadratic_a;
  let axis_position = axis_origin + distance * axis_ray;
  if distance <= 0.0 || axis_position < 0.0 || axis_position > axis_length_squared {
    discard;
  }
  let view_position = ray_direction * distance;
  let axis_surface = start + axis * (axis_position / axis_length_squared);
  let normal = normalize(view_position - axis_surface);
  let element_color = select(
    in.start_color,
    in.end_color,
    axis_position >= axis_length_squared * 0.5,
  );
  var out: FragmentOutput;
  out.color = shade_surface(normal, view_position, element_color);
  out.depth = fragment_depth(view_position);
  return out;
}
