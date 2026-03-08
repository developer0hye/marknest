# Browser PDF and ZIP Download

- Source: html2pdf.js README, https://github.com/eKoopmans/html2pdf.js
- Source: JSZip docs, https://stuk.github.io/jszip/documentation/api_jszip/generate_async.html
- Use browser-side PDF rendering when the Phase 6 requirement is local export without a server.
- `html2pdf.js` supports a browser-only workflow and can output or save PDFs from HTML content.
- The README shows pinned CDN usage for `html2pdf.js`, which keeps the browser bundle simple in a static Trunk app.
- The same README notes a key limitation: rendering currently goes through an image/canvas pipeline, so text quality and file size can be weaker than a vector PDF engine.
- JSZip documents `generateAsync({ type: "blob" })` for download-ready ZIP output; the same packaging step can also be done in Rust/WASM with the repo’s existing `zip` crate.
- For MarkNest, the useful pattern is: render entry HTML in WASM, convert selected HTML to PDF in the browser, then package multiple PDFs into a ZIP for download.
