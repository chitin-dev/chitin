// Keep the entrypoint deliberately small: Svelte owns the UI lifecycle and the
// component owns the browser/WASM resource lifecycle.
import { mount } from "svelte";

import App from "./App.svelte";

import "virtual:uno.css";

// Fail early when the HTML shell and the application entrypoint drift apart.
const target = document.querySelector<HTMLElement>("#app");
if (!target) throw new Error("Missing application mount point");

mount(App, { target });
