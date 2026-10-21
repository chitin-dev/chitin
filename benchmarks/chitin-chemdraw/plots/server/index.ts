/** Loopback-only data/API host, shared by Vite development and compiled production. */
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { createReadStream } from "node:fs";
import { mkdir, readFile, realpath, stat } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import { createGzip } from "node:zlib";
import { getCorpus, listRuns, readRecord } from "./corpus";
import { cacheRoot, plotsRoot, resultsRoot, rowNumber, runDirectory } from "./paths";
import { renderSvg, sourceRevision, watchSources } from "./renderer";
import { benchmarkJob, startBenchmark } from "./benchmark";

const production = process.env.NODE_ENV === "production";
const port = Number(process.env.PORT ?? 5174);
if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("Invalid PORT");
const clients = new Set<ServerResponse>();
let state = "";
let polling = false;
async function broadcast(): Promise<void> {
  if (polling) return;
  polling = true;
  try {
    const next = JSON.stringify({ runs: await listRuns(), sourceRevision: sourceRevision(), job: benchmarkJob() });
    if (next !== state) {
      state = next;
      for (const client of clients) client.write(`data: ${state}\n\n`);
    }
  } finally {
    polling = false;
  }
}
const stopSources = watchSources(() => void broadcast());
const poll = setInterval(() => void broadcast(), 2000);
const heartbeat = setInterval(() => clients.forEach((client) => client.write(": heartbeat\n\n")), 15_000);
await mkdir(cacheRoot, { recursive: true });
const vite = production
  ? undefined
  : await import("vite").then(({ createServer }) =>
      createServer({
        configFile: join(plotsRoot, "vite.config.ts"),
        server: { middlewareMode: true },
        appType: "spa",
      }),
    );

function json(response: ServerResponse, value: unknown, status = 200): void {
  response.writeHead(status, { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store" });
  response.end(JSON.stringify(value));
}
async function body(request: IncomingMessage): Promise<Record<string, unknown>> {
  let content = "";
  for await (const bytes of request) {
    content += bytes;
    if (content.length > 8192) throw new Error("Request body too large");
  }
  return JSON.parse(content || "{}");
}
async function sendFile(response: ServerResponse, path: string, type: string, immutable = false): Promise<void> {
  const root = path.startsWith(resultsRoot + sep) ? resultsRoot : cacheRoot;
  if (!(await realpath(path)).startsWith((await realpath(root)) + sep))
    throw new Error("Artifact escaped its data root");
  const metadata = await stat(path);
  if (!metadata.isFile()) throw new Error("Artifact is not a regular file");
  response.writeHead(200, {
    "Content-Type": type,
    "Content-Length": metadata.size,
    "Cache-Control": immutable ? "private, max-age=31536000, immutable" : "no-cache",
    "X-Content-Type-Options": "nosniff",
    "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'",
  });
  createReadStream(path).pipe(response);
}

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url ?? "/", `http://127.0.0.1:${port}`);
    // Reject cross-site mutations and DNS rebinding; this service is not a remote API.
    const host = request.headers.host?.split(":")[0];
    if (host !== "localhost" && host !== "127.0.0.1") return json(response, { error: "Loopback host required" }, 403);
    if (
      request.headers.origin &&
      ![`http://127.0.0.1:${port}`, `http://localhost:${port}`].includes(request.headers.origin)
    ) {
      return json(response, { error: "Cross-origin requests are not allowed" }, 403);
    }
    if (url.pathname === "/api/events") {
      response.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      });
      await broadcast();
      response.write(`data: ${state}\n\n`);
      clients.add(response);
      request.on("close", () => clients.delete(response));
      return;
    }
    if (url.pathname === "/api/runs")
      return json(response, { runs: await listRuns(), sourceRevision: sourceRevision() });
    if (url.pathname === "/api/benchmark" && request.method === "POST") {
      if (request.headers["x-chitin-client"] !== "plots")
        return json(response, { error: "Client header required" }, 403);
      const options = await body(request);
      if (typeof options.run !== "string") throw new Error("Run ID is required");
      const job = startBenchmark(await getCorpus(options.run), () => void broadcast());
      void broadcast();
      return json(response, job, 202);
    }
    if (url.pathname === "/api/spec") {
      const spec = await readFile(join(plotsRoot, "..", "QUALITY_SPEC.md"), "utf8");
      response.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
      response.end(spec);
      return;
    }
    const segments = url.pathname.split("/").filter(Boolean).map(decodeURIComponent);
    if (segments[0] === "api" && segments[1] === "runs" && segments[2]) {
      const id = segments[2];
      runDirectory(id);
      const corpus = await getCorpus(id);
      if (segments[3] === "saved" && segments[4]) {
        const row = rowNumber(segments[4]);
        if (!corpus.offsets.has(row)) throw new Error("Source row not found");
        return await sendFile(
          response,
          join(corpus.directory, "svg", `${String(row).padStart(8, "0")}.svg`),
          "image/svg+xml",
        );
      }
      if (segments[3] === "data") {
        const etag = `"${corpus.data.revision}"`;
        if (request.headers["if-none-match"] === etag) {
          response.writeHead(304);
          response.end();
          return;
        }
        const content = JSON.stringify(corpus.data);
        response.setHeader("Content-Type", "application/json");
        response.setHeader("Cache-Control", "no-cache");
        response.setHeader("ETag", etag);
        if (request.headers["accept-encoding"]?.includes("gzip")) {
          response.setHeader("Content-Encoding", "gzip");
          response.setHeader("Vary", "Accept-Encoding");
          const gzip = createGzip();
          gzip.pipe(response);
          gzip.end(content);
        } else response.end(content);
        return;
      }
      if (segments[3] === "records" && segments[4])
        return json(response, await readRecord(corpus, rowNumber(segments[4])));
      if (segments[3] === "render" && segments[4] && request.method === "POST") {
        if (request.headers["x-chitin-client"] !== "plots")
          return json(response, { error: "Client header required" }, 403);
        const options = await body(request);
        if (options.mode !== "current" && options.mode !== "snapshot") throw new Error("Invalid renderer mode");
        return json(response, await renderSvg(corpus, rowNumber(segments[4]), options.mode));
      }
    }
    if (segments[0] === "api" && segments[1] === "cache" && segments.length === 5) {
      if (!/^[a-f0-9]{64}$/.test(segments[2])) throw new Error("Invalid renderer fingerprint");
      runDirectory(segments[3]);
      if (!/^[1-9][0-9]*-[a-f0-9]{64}$/.test(segments[4])) throw new Error("Invalid artifact identity");
      return await sendFile(
        response,
        join(cacheRoot, "svg", segments[2], segments[3], `${segments[4]}.svg`),
        "image/svg+xml",
        true,
      );
    }
    if (url.pathname.startsWith("/api/")) return json(response, { error: "Endpoint not found" }, 404);
    if (vite) return vite.middlewares(request, response);
    const clientRoot = join(plotsRoot, "dist", "client");
    const path = resolve(clientRoot, "." + decodeURIComponent(url.pathname));
    if (!path.startsWith(clientRoot + sep) && path !== clientRoot) throw new Error("Invalid asset path");
    const extension = path.split(".").at(-1);
    const contentType = extension === "js" ? "application/javascript" : extension === "css" ? "text/css" : "text/html";
    const asset = await stat(path)
      .then((metadata) => metadata.isFile())
      .catch(() => false);
    if (!asset && (url.pathname.startsWith("/assets/") || ["js", "css", "svg", "png", "map"].includes(extension ?? "")))
      return json(response, { error: "Asset not found" }, 404);
    // SPA fallback only for navigation routes, never for missing script/worker chunks.
    const file = asset ? path : join(clientRoot, "index.html");
    response.writeHead(200, {
      "Content-Type": contentType,
      "Cache-Control": contentType === "text/html" ? "no-cache" : "public, max-age=31536000, immutable",
    });
    createReadStream(file).pipe(response);
  } catch (error) {
    if (!response.headersSent) json(response, { error: error instanceof Error ? error.message : String(error) }, 400);
    else response.end();
  }
});
server.listen(port, "127.0.0.1", () =>
  console.log(`Chitin plots → http://127.0.0.1:${port} (${production ? "production" : "development"})`),
);
for (const signal of ["SIGTERM", "SIGINT"] as const)
  process.on(signal, () => {
    clearInterval(poll);
    clearInterval(heartbeat);
    stopSources();
    clients.forEach((client) => client.end());
    void vite?.close();
    server.close(() => process.exit(0));
  });
