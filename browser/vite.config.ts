import { svelte } from "@sveltejs/vite-plugin-svelte";
import UnoCSS from "unocss/vite";
import { defineConfig, searchForWorkspaceRoot } from "vite";

// The WASM package lives beside the browser app, so Vite must be allowed to
// serve that workspace-local dependency during development.
const repositoryRoot = new URL("..", import.meta.url).pathname;

export default defineConfig({
  // UnoCSS runs before Svelte so generated utility styles are available to
  // the compiled component without a hand-written stylesheet.
  plugins: [UnoCSS(), svelte()],
  server: {
    fs: {
      allow: [searchForWorkspaceRoot(repositoryRoot), repositoryRoot],
    },
  },
});
