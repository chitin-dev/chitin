---
name: wgpui
description:
  Use when adding, debugging, or reviewing GPUI panels that embed raw wgpu
  rendering through the chitin-dev/gpui-wgpu fork. Trigger for WGPUI, gpui-wgpu,
  WgpuSurfaceHandle, wgpu_surface, create_wgpu_surface, GPUI + WGPU integration,
  triple-buffered WGPU panels, or Chitin's experimental 3D structure-viewer
  rendering path.
---

# WGPUI

## Purpose

Use `chitin-dev/gpui-wgpu` when Chitin needs GPUI layout and application state,
but a panel region must be rendered by raw `wgpu`. Treat GPUI as the owner of
windows, panels, input, and composition; treat `wgpu` as the owner of scene
resources, shaders, command encoders, and frame rendering.

Primary local example:

```text
crates/chitin-desktop/examples/chitin-wgpu-desktop.rs
crates/chitin-desktop/examples/chitin-wgpu/cube.rs
crates/chitin-desktop/examples/chitin-wgpu/cube.wgsl
```

## Dependency Shape

Use the fork as GPUI itself:

```toml
gpui = { package = "gpui-ce", git = "https://github.com/chitin-dev/gpui-wgpu" }
wgpu = { version = "30", default-features = false, features = ["vulkan", "wgsl"] }
```

Keep `wgpu` features native-only unless the task explicitly targets WASM. Do not
add `web-sys`, `wasm-bindgen`, or WGPU web features to Chitin's native desktop
crates just to satisfy native builds.

## Integration Pattern

Create the surface from the GPUI `Window`:

```rust
let surface = window.create_wgpu_surface(width, height, wgpu::TextureFormat::Rgba8UnormSrgb);
```

Store `Option<WgpuSurfaceHandle>` in the GPUI view. It may be `None` when the
backend cannot create a WGPU surface, so render a fallback instead of
unwrapping.

Create renderer state lazily after the first back buffer exists. Clone the
surface device and queue into the renderer, and create GPU resources from that
same device. Avoid separate devices unless the design explicitly solves
inter-device sharing.

Render inside the view's frame path or from a controlled render loop:

```rust
let _submit_guard = surface.submit_guard();
let Some((view, (width, height))) = surface.back_view_with_size() else {
  return;
};

// Resize size-dependent resources when width or height changes.
let submission_index = renderer.render(&view);
drop(view);
surface.present_synced_silent(submission_index);
```

Use `submit_guard()` across encoding, queue submission, and presentation. It
coordinates with resize/reconfigure work on the shared GPUI WGPU device.

Use `present_synced_silent()` when the GPUI view already calls
`window.request_animation_frame()`. Use `present_synced()` only when the render
thread itself should trigger redraws.

Place the element in GPUI layout with `wgpu_surface(surface)`:

```rust
wgpu_surface(surface)
  .absolute()
  .inset_0()
  .defer_resize_until_mouse_up(true)
```

The element is just GPUI layout/composition. It does not replace the renderer,
camera, mesh generation, or shader pipeline.

## Renderer Structure

Keep a clear boundary:

- GPUI view: owns `WgpuSurfaceHandle`, focus/input state, panel chrome, FPS
  overlay, and repaint scheduling.
- WGPU renderer: owns `RenderPipeline`, buffers, bind groups, depth targets,
  camera uniforms, and draw calls.
- Domain crates: own molecular data and representation generation. Do not make
  domain crates depend on GPUI.

For shader code, prefer external `.wgsl` files and `include_str!` for examples
or small renderers. This keeps Rust focused on integration and keeps shader
iteration readable.

For WGPU matrices, remember WGPU uses a DirectX-style depth range. If using
`glam`, prefer a matching projection helper such as
`glam::camera::rh::proj::directx::perspective`.

## Performance Checks

For low FPS or camera-move jitter, inspect these first:

- Reuse pipelines, bind groups, static buffers, and depth textures; do not
  recreate them every frame.
- Recreate depth and other size-dependent textures only when
  `back_view_with_size()` reports a size change.
- Use `present_synced_silent()` with `request_animation_frame()` to avoid
  double-driving full-window redraws.
- Keep the WGPU surface stable during panel drags with
  `.defer_resize_until_mouse_up(true)`.
- Avoid calling `cx.notify()` from high-frequency event paths unless the frame
  loop actually needs it.
- Keep camera movement in uniform updates, not mesh rebuilds.

## Validation

For Chitin's current example, prefer:

```bash
cargo fmt -p chitin-desktop
cargo check --example chitin-wgpu-desktop --offline
```

Use `cargo run --example chitin-wgpu-desktop -- .` when visual validation is
required. If the repo uses a local patched GPUI fork through a path dependency,
avoid broad formatting commands that descend into that fork unless it is known
to format cleanly.

## Conclusion of Current Experiment

### Achieved

The WGPUI experiment validates that a `<GPUI layout + wgpu rendering>` split is
practical for embedding interactive 3D viewports inside Chitin's document-area
panel system. The codebase has three well-separated layers:

| Layer | Crate | Responsibility |
|-------|-------|---------------|
| Framework-neutral helpers | `chitin-wgpu` | `ClearRenderer`, `DepthTarget`, `RenderTargetSize`, `ViewerCamera`, `ViewportDrag` — reusable GPU and viewport math with no GPUI dependency |
| GPUI panel adapter | `chitin-desktop` `wgpu_panel` | `WgpuPanelScene` trait, `ChitinWgpuDocumentPanel` view, input routing (orbit/pan/zoom), FPS overlay, render-loop scheduling via `request_animation_frame` |
| Example scene | `chitin-wgpu-desktop` example | `ExampleCubeScene` with full `wgpu::RenderPipeline`, vertex/index/uniform buffers, bind groups, and a WGSL shader; launched as a file-picker desktop app |

The panel integrates into the document panel tree as a tab holding
`WgpuInteractive` (an `AnyView` variant of `DocumentPanelContent`). The
`ChitinApp::new_with_wgpu_document_panel()` constructor wires it into the app
shell for development.

### Key Design Decisions Validated

1. **`chitin-wgpu` is deliberately dependency-free of GPUI.** Camera math,
   render-target helpers, and clear-pipeline logic are usable from tests or
   headless contexts without launching GPUI. This aligns with the roadmap
   principle in Phase 2.5 ("these crates must not depend on GPUI").

2. **The `WgpuPanelScene` trait cleanly separates chrome from content.** GPUI
   owns input dispatch, surface lifecycle, repaint scheduling, and overlay
   chrome; scene implementors own pipelines, shaders, and draw calls. No scene
   code references GPUI.

3. **The document panel tree can host arbitrary GPU content.** The existing
   `DocumentPanelContent` enum already has a catch-all `WgpuInteractive` variant
   via `AnyView`, so no structural changes to the panel system were required.

4. **Vulkan + WGSL on native Linux works as expected.** The spinning-cube example
   compiles with `wgpu` v30 (`vulkan` + `wgsl` features), runs under a file-tree
   desktop, and achieves smooth frame rates.

5. **Fork surface API is minimal and stable.** `create_wgpu_surface`,
   `submit_guard`, `back_view_with_size`, `present_synced_silent`, and
   `wgpu_surface(element)` are the only GPUI additions needed. No GPUI-internal
   rendering changes are required.

### Remaining Open Questions

The roadmap decision at `ROADMAP.md:284` remains unresolved:

> Whether 3D rendering should be implemented directly in GPUI, embedded through
> a dedicated renderer, or delegated to an external visualization engine.

This experiment demonstrates the "embedded through a dedicated renderer" option,
but Chitin has not yet committed to it. The alternative paths (GPUI-native
rendering or delegating to an external engine such as `three-d` or `kiss3d`)
would each imply different dependency, portability, and integration costs.

### Practical Status

- The example compiles under `cargo check --workspace --locked` and runs on
  Vulkan-capable Linux desktops via `cargo run --example chitin-wgpu-desktop`.
- CI does not run the example (no Vulkan driver on Ubuntu runners), so visual
  regressions are caught only by manual launch.
- The `#[allow(dead_code)]` annotation on `ChitinWgpuDocumentPanel::new()`
  signals that the clear-scene path is present but not yet wired into production
  code paths outside the example.
