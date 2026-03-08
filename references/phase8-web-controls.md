# Phase 8 Web Controls

- Source: Axum multipart docs, https://docs.rs/axum/latest/axum/extract/struct.Multipart.html
- Source: MDN FormData, https://developer.mozilla.org/en-US/docs/Web/API/FormData
- Source: MDN Blob, https://developer.mozilla.org/en-US/docs/Web/API/Blob
- Use multipart requests when the browser needs to upload raw ZIP bytes plus optional text fields like CSS or header/footer templates.
- Keep the ZIP as a real file/blob field instead of serializing it into JSON so large archives do not balloon in memory.
- Raise the Axum body limit for multipart uploads so local fallback requests can accept realistic workspace ZIP sizes.
- Reuse the same render option model for preview, browser export, debug bundles, and server fallback to avoid mode drift.
- Build the debug bundle in-browser as a ZIP that contains rendered HTML plus JSON diagnostics, so it remains downloadable even without the server.
