# Trunk WASM Browser Client

- Source: Trunk guide, JavaScript snippets and startup event docs, https://trunkrs.dev/guide/assets/index.html and https://trunkrs.dev/guide/advanced/javascript/index.html
- Source: `wasm-bindgen` type conversion docs for numeric slices and `Vec<T>`, https://rustwasm.github.io/wasm-bindgen/reference/types/number-slices.html and https://rustwasm.github.io/wasm-bindgen/reference/types/boxed-slices.html
- Use Trunk as the static app bundler when the browser UI is mostly HTML/CSS/JS and Rust provides exported WASM functions.
- Put the browser shell in static assets and expose Rust functions through `#[wasm_bindgen]` so JavaScript can call them after Trunk startup.
- Accept ZIP bytes from JavaScript as numeric slices or vectors and return structured data through `serde_wasm_bindgen`.
- Keep the browser flow client-only: upload bytes in JS, call WASM analyze/render functions, then write preview HTML into the DOM or an iframe.
- This matches MarkNest Phase 5 because the browser needs ZIP upload, entry selection, warnings, and preview without a server.
