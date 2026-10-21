import { derive } from "./lib/derive";

/** Network decoding, sorting and distribution assembly never block pointer interactions. */
let controller: AbortController | undefined;
self.onmessage = async (event: MessageEvent<{ id: number; run: string; revision: string }>) => {
  controller?.abort();
  controller = new AbortController();
  const id = event.data.id;
  try {
    const response = await fetch(`/api/runs/${encodeURIComponent(event.data.run)}/data`, { signal: controller.signal });
    if (!response.ok) throw new Error((await response.json()).error ?? "Dataset load failed");
    const data = await response.json();
    const derived = derive(data);
    self.postMessage({ id, data, derived });
  } catch (error) {
    if (error instanceof Error && error.name === "AbortError") return;
    self.postMessage({ id, error: error instanceof Error ? error.message : String(error) });
  }
};
