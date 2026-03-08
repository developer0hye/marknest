# Phase 8 Per-Side Margins

- Source: Chrome DevTools Protocol `Page.printToPDF`, https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-printToPDF
- Source: `html2pdf.js` README, https://github.com/eKoopmans/html2pdf.js
- `Page.printToPDF` exposes `marginTop`, `marginBottom`, `marginLeft`, and `marginRight` separately, so Chromium fallback should keep side-specific values instead of flattening to one number.
- `html2pdf.js` accepts `margin` as either a single number or an array, and the README documents the array order as `[top, left, bottom, right]`.
- For MarkNest, keep `--margin` as a convenience shorthand that fans out to all sides, then let side-specific overrides win.
- Use the same four explicit fields in web/server JSON payloads and debug artifacts so browser export, fallback export, and diagnostics stay aligned.
