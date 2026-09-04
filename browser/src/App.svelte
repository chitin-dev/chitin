<script lang="ts">
  // Browser-owned resources live in this component so their setup and cleanup
  // follow the same lifecycle as the canvas element.
  import { onMount } from "svelte";

  import initWasm, { create_viewer, type MoleculeViewer } from "../../crates/chitin-wasm/pkg/chitin_wasm.js";

  // A small built-in structure makes the viewer useful before a file is chosen.
  const SAMPLE_PDB = `HEADER    CHITIN WEB SAMPLE
ATOM      1  N   GLY A   1      -1.230   0.120   0.050  1.00 10.00           N
ATOM      2  CA  GLY A   1       0.000   0.000   0.000  1.00 10.00           C
ATOM      3  C   GLY A   1       0.710   1.320  -0.080  1.00 10.00           C
ATOM      4  O   GLY A   1       0.120   2.390  -0.120  1.00 10.00           O
ATOM      5  N   ALA A   2       2.030   1.230  -0.090  1.00 10.00           N
ATOM      6  CA  ALA A   2       2.850   2.420  -0.180  1.00 10.00           C
ATOM      7  CB  ALA A   2       3.020   3.110   1.180  1.00 10.00           C
ATOM      8  C   ALA A   2       4.210   2.040  -0.720  1.00 10.00           C
ATOM      9  O   ALA A   2       4.940   2.900  -1.210  1.00 10.00           O
ATOM     10  N   SER A   3       4.560   0.770  -0.650  1.00 10.00           N
ATOM     11  CA  SER A   3       5.830   0.270  -1.160  1.00 10.00           C
ATOM     12  CB  SER A   3       6.740   0.030   0.030  1.00 10.00           C
ATOM     13  OG  SER A   3       6.190  -0.920   0.930  1.00 10.00           O
END
`;

  // UI state is kept separate from the WASM viewer so loading failures can be
  // reported without losing the rest of the page state.
  let canvas: HTMLCanvasElement;
  let viewport: HTMLElement;
  let viewer: MoleculeViewer | undefined;
  let frameHandle = 0;
  let resizeObserver: ResizeObserver | undefined;
  let disposed = false;
  // Only the latest file request may update the viewer or its metadata.
  let loadGeneration = 0;
  let statusKind: "working" | "ready" | "error" = "working";
  let statusText = "Initializing WebGPU";
  let structureName = "Web sample";
  let structureSummary = "Loading…";
  let representation = "ball-and-stick";
  let dropVisible = false;

  // WebGPU can only be initialized after Svelte has mounted the canvas.
  onMount(() => {
    disposed = false;
    void start();
    resizeObserver = new ResizeObserver(resizeCanvas);
    resizeObserver.observe(viewport);

    return () => {
      // `start` may still be suspended in WASM or WebGPU initialization.
      disposed = true;
      loadGeneration += 1;
      cancelAnimationFrame(frameHandle);
      resizeObserver?.disconnect();
      viewer?.free();
    };
  });

  // Initializes the WASM module, creates the GPU viewer, and loads the sample.
  async function start(): Promise<void> {
    try {
      if (!("gpu" in navigator)) {
        throw new Error("WebGPU is unavailable. Use a current browser with WebGPU enabled.");
      }
      await initWasm();
      if (disposed) return;
      resizeCanvas();
      const nextViewer = await create_viewer(canvas);
      if (disposed) {
        nextViewer.free();
        return;
      }
      viewer = nextViewer;
      structureSummary = nextViewer.load_structure(new TextEncoder().encode(SAMPLE_PDB), "pdb");
      setReady("WebGPU ready");
      frameHandle = requestAnimationFrame(renderFrame);
    } catch (error) {
      if (disposed) return;
      setError(error);
    }
  }

  // Keep presenting frames while the viewer is healthy; the error path stops
  // the loop so a failed device does not produce an unbounded callback chain.
  function renderFrame(): void {
    if (disposed || !viewer) return;
    try {
      viewer.render();
      frameHandle = requestAnimationFrame(renderFrame);
    } catch (error) {
      if (disposed) return;
      setError(error);
    }
  }

  // Match the drawing buffer to the CSS viewport while capping high-DPI work.
  function resizeCanvas(): void {
    const scale = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(viewport.clientWidth * scale));
    const height = Math.max(1, Math.round(viewport.clientHeight * scale));
    if (canvas.width === width && canvas.height === height) return;
    canvas.width = width;
    canvas.height = height;
    viewer?.resize(width, height);
  }

  // JavaScript owns file selection and decoding; Rust receives only the bytes
  // and the format needed by its existing structure parsers.
  async function loadFile(file: File): Promise<void> {
    const generation = ++loadGeneration;

    try {
      const format = structureFormat(file.name);
      setWorking(`Parsing ${file.name}`);
      const bytes = new Uint8Array(await file.arrayBuffer());

      // A newer selection may have started while this file was being read.
      if (disposed || generation !== loadGeneration) return;
      if (!viewer) throw new Error("The WebGPU viewer is not initialized");

      const summary = viewer.load_structure(bytes, format);
      if (disposed || generation !== loadGeneration) return;

      structureSummary = summary;
      structureName = file.name;
      setReady("Structure loaded");
    } catch (error) {
      if (disposed || generation !== loadGeneration) return;
      setError(error);
    }
  }

  // The hidden file input remains keyboard-accessible through its label.
  function handleFileInput(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (file) void loadFile(file);
  }

  // Rebuilding the renderer is handled by the WASM bridge after a mode change.
  function handleRepresentation(): void {
    try {
      viewer?.set_representation(representation);
    } catch (error) {
      setError(error);
    }
  }

  // Pointer coordinates are canvas-local so camera movement is independent of
  // the canvas position in the surrounding layout.
  function handlePointerDown(event: PointerEvent): void {
    canvas.setPointerCapture(event.pointerId);
    viewer?.pointer_down(event.button, event.shiftKey, event.offsetX, event.offsetY);
  }

  function handlePointerUp(event: PointerEvent): void {
    if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
    viewer?.pointer_up();
  }

  function handleWheel(event: WheelEvent): void {
    event.preventDefault();
    viewer?.zoom(event.deltaY);
  }

  // Drag-and-drop state is intentionally visual-only; parsing still goes
  // through loadFile so picker and drop behavior stay consistent.
  function showDrop(event: DragEvent): void {
    event.preventDefault();
    dropVisible = true;
  }

  function hideDrop(event: DragEvent): void {
    event.preventDefault();
    dropVisible = false;
  }

  function handleDrop(event: DragEvent): void {
    hideDrop(event);
    const file = event.dataTransfer?.files[0];
    if (file) void loadFile(file);
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key.toLowerCase() === "r" && event.target === document.body) viewer?.reset_camera();
  }

  // File extensions are mapped to the parser names exported by the Rust bridge.
  function structureFormat(name: string): "pdb" | "mmcif" {
    const extension = name.split(".").pop()?.toLowerCase();
    if (extension === "pdb" || extension === "ent") return "pdb";
    if (extension === "cif" || extension === "mmcif") return "mmcif";
    throw new Error("Unsupported file type. Choose a .pdb, .ent, .cif, or .mmcif file.");
  }

  // Keep status transitions centralized so every asynchronous operation uses
  // the same visual vocabulary.
  function setWorking(message: string): void {
    statusKind = "working";
    statusText = message;
  }

  function setReady(message: string): void {
    statusKind = "ready";
    statusText = message;
  }

  function setError(error: unknown): void {
    cancelAnimationFrame(frameHandle);
    statusKind = "error";
    statusText = error instanceof Error ? error.message : String(error);
    console.error(error);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- The shell contains the persistent application chrome and the viewer. -->
<main class="shell">
  <header class="topbar">
    <a class="brand" href="/" aria-label="Chitin home">
      <span class="brand-mark" aria-hidden="true"></span>
      <span>CHITIN</span>
    </a>
    <div class={`status ${statusKind === "error" ? "text-[#ff9d91]" : ""}`} role="status">
      <span class={`status-dot status-${statusKind}`}></span>
      <span>{statusText}</span>
    </div>
    <a class="source-link" href="https://github.com/chitin-dev/chitin" target="_blank" rel="noreferrer">Source ↗</a>
  </header>

  <section class="workspace">
    <aside class="panel">
      <div>
        <p class="eyebrow">MOLECULAR VIEWER</p>
        <h1 class="headline">Structure,<br /><em class="headline-accent">in motion.</em></h1>
        <p class="lede">Native Chitin parsing and rendering, compiled to WebAssembly and presented through WebGPU.</p>
      </div>

      <div class="controls">
        <label class="file-control">
          <span class="control-label">Load structure</span>
          <strong class="mt-2 text-xs font-medium">Choose PDB or mmCIF</strong>
          <input
            class="pointer-events-none absolute h-px w-px opacity-0"
            type="file"
            accept=".pdb,.ent,.cif,.mmcif"
            onchange={handleFileInput}
          />
        </label>

        <label class="select-control" for="representation">
          <span class="control-label">Representation</span>
          <select class="select-input" id="representation" bind:value={representation} onchange={handleRepresentation}>
            <option value="ball-and-stick">Ball &amp; stick</option>
            <option value="stick">Stick</option>
            <option value="sphere">Space filling</option>
          </select>
        </label>

        <button class="reset-button" id="reset-camera" type="button" onclick={() => viewer?.reset_camera()}>
          Reset camera <span class="key-cap">R</span>
        </button>
      </div>

      <div class="structure-meta">
        <span class="structure-name" id="structure-name">{structureName}</span>
        <span>{structureSummary}</span>
      </div>
    </aside>

    <!-- The canvas owns rendering; the surrounding section owns drop events. -->
    <section
      class="viewport"
      aria-label="Molecule viewport and file drop area"
      bind:this={viewport}
      ondragenter={showDrop}
      ondragover={showDrop}
      ondragleave={hideDrop}
      ondrop={handleDrop}
    >
      <canvas
        class="viewport-canvas"
        bind:this={canvas}
        aria-label="Interactive molecular structure"
        oncontextmenu={(event) => event.preventDefault()}
        ondblclick={() => viewer?.reset_camera()}
        onpointerdown={handlePointerDown}
        onpointermove={(event) => viewer?.pointer_move(event.offsetX, event.offsetY)}
        onpointerup={handlePointerUp}
        onpointercancel={() => viewer?.pointer_up()}
        onwheel={handleWheel}
      ></canvas>
      <div class="viewport-label"><span class="text-acid">WEBGPU</span> LIVE</div>
      <div class="interaction-hint">Drag to rotate · Shift-drag to pan · Scroll to zoom</div>
      <div class={`drop-overlay ${dropVisible ? "drop-visible" : ""}`}>Drop structure to inspect</div>
    </section>
  </section>
</main>
