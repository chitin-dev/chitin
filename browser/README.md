# Chitin browser viewer

The browser application uses Svelte and TypeScript. `chitin-wasm` supplies the generated WebAssembly bindings, while
pnpm owns JavaScript dependencies and Oxc provides formatting and linting.

```sh
pnpm install
pnpm wasm:build
pnpm dev
```

From the repository root, the same rebuild-and-run flow is available as:

```sh
just browser-dev
```

Deno can run the same project tasks without creating a second dependency graph:

```sh
deno task setup
deno task dev
deno task validate
```

Run `pnpm validate` before committing browser changes. The generated package at `../crates/chitin-wasm/pkg` and Vite's
`dist` directory are intentionally ignored by Git.
