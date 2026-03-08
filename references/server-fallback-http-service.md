# Server Fallback HTTP Service

- Source: Axum docs, https://docs.rs/axum/latest/axum/
- Source: Tower HTTP CORS layer docs, https://docs.rs/tower-http/latest/tower_http/cors/
- Use a small local HTTP service when the browser UI needs a fallback path but the rendering engine already exists on the machine.
- Keep the request body raw when the browser already has ZIP bytes in memory; avoid JSON arrays of bytes for large payloads.
- Return binary responses directly with `application/pdf` and `application/zip` so the browser can trigger downloads without an extra translation step.
- Add permissive local CORS for the Trunk dev server so the browser app can call the fallback service during development.
- For MarkNest, the server fallback should reuse the same ZIP analysis and HTML rendering path as the browser so entry selection and diagnostics stay aligned.
