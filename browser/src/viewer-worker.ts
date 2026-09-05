import initWasm, { create_viewer, type MoleculeViewer } from "../../crates/chitin-wasm/pkg/chitin_wasm.js";

type WorkerCommand =
  | { type: "init"; canvas: OffscreenCanvas; width: number; height: number }
  | { type: "load"; generation: number; bytes: ArrayBuffer; format: "pdb" | "mmcif" }
  | { type: "resize"; width: number; height: number }
  | { type: "pointer-down"; button: number; shiftKey: boolean; x: number; y: number }
  | { type: "pointer-move"; x: number; y: number }
  | { type: "pointer-up" }
  | { type: "zoom"; deltaY: number }
  | { type: "reset-camera" }
  | { type: "representation"; value: string };

type WorkerResponse =
  | { type: "ready" }
  | { type: "loaded"; generation: number; summary: string }
  | { type: "error"; generation?: number; message: string };

// The worker global is typed locally so the browser app can keep DOM and
// Worker-specific TypeScript libraries separate in the shared tsconfig.
const workerScope = globalThis as unknown as {
  addEventListener(type: "message", listener: (event: MessageEvent<WorkerCommand>) => void): void;
  postMessage(message: WorkerResponse): void;
};

let viewer: MoleculeViewer | undefined;

// Keep all parsing, bond inference, scene extraction, and GPU renderer work in
// this worker so the browser main thread remains available for UI input.
workerScope.addEventListener("message", (event) => {
  void handleCommand(event.data);
});

async function handleCommand(command: WorkerCommand): Promise<void> {
  try {
    if (command.type === "init") {
      await initWasm();
      viewer = await create_viewer(command.canvas);
      viewer.resize(command.width, command.height);
      viewer.render();
      workerScope.postMessage({ type: "ready" });
      return;
    }

    if (!viewer) throw new Error("The WebGPU viewer worker is not initialized");

    switch (command.type) {
      case "load": {
        const summary = viewer.load_structure(new Uint8Array(command.bytes), command.format);
        viewer.render();
        workerScope.postMessage({ type: "loaded", generation: command.generation, summary });
        break;
      }
      case "resize":
        viewer.resize(command.width, command.height);
        viewer.render();
        break;
      case "pointer-down":
        viewer.pointer_down(command.button, command.shiftKey, command.x, command.y);
        break;
      case "pointer-move":
        viewer.pointer_move(command.x, command.y);
        viewer.render();
        break;
      case "pointer-up":
        viewer.pointer_up();
        break;
      case "zoom":
        viewer.zoom(command.deltaY);
        viewer.render();
        break;
      case "reset-camera":
        viewer.reset_camera();
        viewer.render();
        break;
      case "representation":
        viewer.set_representation(command.value);
        viewer.render();
        break;
    }
  } catch (error) {
    workerScope.postMessage({
      type: "error",
      generation: command.type === "load" ? command.generation : undefined,
      message: error instanceof Error ? error.message : String(error),
    });
  }
}
