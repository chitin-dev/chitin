<script lang="ts">
  // Browser-owned resources live in this component so their setup and cleanup
  // follow the same lifecycle as the canvas element.
  import { onMount } from "svelte";

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
  type WorkerResponse =
    | { type: "ready" }
    | { type: "loaded"; generation: number; summary: string }
    | { type: "error"; generation?: number; message: string };

  let viewerWorker: Worker | undefined;
  let workerReady = false;
  let resizeObserver: ResizeObserver | undefined;
  let disposed = false;
  // Only the latest file request may update the viewer or its metadata.
  let loadGeneration = 0;
  const pendingStructureNames = new Map<number, string>();
  let statusKind: "working" | "ready" | "error" = "working";
  let statusText = "Initializing WebGPU";
  let structureName = "Web sample";
  let structureSummary = "Loading…";
  let atomStyle = "ball-and-stick";
  let cartoonEnabled = false;
  let dropVisible = false;

  // WebGPU can only be initialized after Svelte has mounted the canvas.
  onMount(() => {
    disposed = false;
    start();
    resizeObserver = new ResizeObserver(resizeCanvas);
    resizeObserver.observe(viewport);

    return () => {
      // `start` may still be suspended in WASM or WebGPU initialization.
      disposed = true;
      loadGeneration += 1;
      resizeObserver?.disconnect();
      viewerWorker?.terminate();
    };
  });

  // Transfers the canvas to a worker, which owns WASM, WebGPU, and rendering.
  function start(): void {
    try {
      if (!("gpu" in navigator)) {
        throw new Error("WebGPU is unavailable. Use a current browser with WebGPU enabled.");
      }
      // Set the real drawing-buffer size before transferring the canvas. The
      // default HTML canvas size is only 300x150 and would make the viewer
      // appear blurry when stretched across the viewport.
      resizeCanvas();
      const worker = new Worker(new URL("./viewer-worker.ts", import.meta.url), { type: "module" });
      worker.addEventListener("message", handleWorkerMessage);
      viewerWorker = worker;
      const offscreenCanvas = canvas.transferControlToOffscreen();
      worker.postMessage(
        {
          type: "init",
          canvas: offscreenCanvas,
          width: Math.max(1, canvas.width),
          height: Math.max(1, canvas.height),
        },
        [offscreenCanvas],
      );
    } catch (error) {
      setError(error);
    }
  }

  // Apply worker results only while this component remains mounted. The load
  // generation rejects responses for older file selections.
  function handleWorkerMessage(event: MessageEvent<WorkerResponse>): void {
    const message = event.data;
    if (message.type === "ready") {
      workerReady = true;
      pendingStructureNames.set(0, "Web sample");
      const sampleBytes = new TextEncoder().encode(SAMPLE_PDB);
      viewerWorker?.postMessage({ type: "load", generation: 0, bytes: sampleBytes.buffer, format: "pdb" }, [
        sampleBytes.buffer,
      ]);
    } else if (message.type === "loaded") {
      if (disposed || message.generation !== loadGeneration) return;
      structureSummary = message.summary;
      structureName = pendingStructureNames.get(message.generation) ?? structureName;
      pendingStructureNames.delete(message.generation);
      setReady("Structure loaded");
    } else if (message.type === "error") {
      if (disposed || (message.generation !== undefined && message.generation !== loadGeneration)) return;
      setError(new Error(message.message));
    }
  }

  // Match the drawing buffer to the CSS viewport while capping high-DPI work.
  function resizeCanvas(): void {
    const scale = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(viewport.clientWidth * scale));
    const height = Math.max(1, Math.round(viewport.clientHeight * scale));
    if (canvas.width === width && canvas.height === height) return;
    if (workerReady) {
      viewerWorker?.postMessage({ type: "resize", width, height });
      return;
    }
    canvas.width = width;
    canvas.height = height;
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
      if (!viewerWorker || !workerReady) throw new Error("The WebGPU viewer worker is not initialized");

      pendingStructureNames.set(generation, file.name);
      viewerWorker.postMessage({ type: "load", generation, bytes: bytes.buffer, format }, [bytes.buffer]);
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

  // Atom and polymer layers are updated independently by the WASM bridge.
  function handleAtomStyle(): void {
    viewerWorker?.postMessage({ type: "atom-style", value: atomStyle });
  }

  function handleCartoonEnabled(): void {
    viewerWorker?.postMessage({ type: "cartoon-enabled", enabled: cartoonEnabled });
  }

  // Pointer coordinates are canvas-local so camera movement is independent of
  // the canvas position in the surrounding layout.
  function handlePointerDown(event: PointerEvent): void {
    canvas.setPointerCapture(event.pointerId);
    viewerWorker?.postMessage({
      type: "pointer-down",
      button: event.button,
      shiftKey: event.shiftKey,
      x: event.offsetX,
      y: event.offsetY,
    });
  }

  function handlePointerUp(event: PointerEvent): void {
    if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
    viewerWorker?.postMessage({ type: "pointer-up" });
  }

  function handlePointerMove(event: PointerEvent): void {
    viewerWorker?.postMessage({ type: "pointer-move", x: event.offsetX, y: event.offsetY });
  }

  function handlePointerCancel(): void {
    viewerWorker?.postMessage({ type: "pointer-up" });
  }

  function handleWheel(event: WheelEvent): void {
    event.preventDefault();
    viewerWorker?.postMessage({ type: "zoom", deltaY: event.deltaY });
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
    if (event.key.toLowerCase() === "r" && event.target === document.body) {
      viewerWorker?.postMessage({ type: "reset-camera" });
    }
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

        <label class="select-control" for="atom-style">
          <span class="control-label">Atom style</span>
          <select class="select-input" id="atom-style" bind:value={atomStyle} onchange={handleAtomStyle}>
            <option value="ball-and-stick">Ball &amp; stick</option>
            <option value="stick">Stick</option>
            <option value="sphere">Space filling</option>
          </select>
        </label>

        <label class="flex cursor-pointer items-center gap-2 text-xs">
          <input
            class="h-4 w-4 accent-sky-500"
            type="checkbox"
            bind:checked={cartoonEnabled}
            onchange={handleCartoonEnabled}
          />
          <span>Polymer cartoon</span>
        </label>

        <button
          class="reset-button"
          id="reset-camera"
          type="button"
          onclick={() => viewerWorker?.postMessage({ type: "reset-camera" })}
        >
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
        ondblclick={() => viewerWorker?.postMessage({ type: "reset-camera" })}
        onpointerdown={handlePointerDown}
        onpointermove={handlePointerMove}
        onpointerup={handlePointerUp}
        onpointercancel={handlePointerCancel}
        onwheel={handleWheel}
      ></canvas>
      <div class="viewport-label"><span class="text-acid">WEBGPU</span> LIVE</div>
      <div class="interaction-hint">Drag to rotate · Shift-drag to pan · Scroll to zoom</div>
      <div class={`drop-overlay ${dropVisible ? "drop-visible" : ""}`}>Drop structure to inspect</div>
    </section>
  </section>
</main>
