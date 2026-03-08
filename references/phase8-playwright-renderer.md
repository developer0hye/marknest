# Phase 8 Playwright Renderer

- Source: Playwright `page.pdf()` docs, https://playwright.dev/docs/api/class-page#page-pdf
- Source: Playwright `browserType.launch()` docs, https://playwright.dev/docs/api/class-browsertype#browser-type-launch
- Source: Playwright library install docs, https://playwright.dev/docs/library#browser-downloads
- `page.pdf()` is Chromium-only, so the native/server high-quality path should use `chromium.launch(...)`.
- `browserType.launch()` supports `executablePath`, which lets the product keep using the existing Chrome/Chromium discovery path instead of forcing a bundled browser download.
- The Playwright library package does not require automatic browser downloads for this slice because the runtime already has a browser path contract (`MARKNEST_BROWSER_PATH` plus platform defaults).
- Keep the existing `window.__MARKNEST_RENDER_STATUS__` wait policy and validation/warning surface, but move the actual page load and PDF generation to Playwright.
- Prefer a repo-local Node runtime package with a pinned Playwright version and lockfile so Docker and local runs resolve the same JS dependency.
- Extend browser discovery to macOS and Linux defaults (`/Applications/...`, `/usr/bin/chromium`, `/usr/bin/google-chrome`) because the current resolver is Windows-only.
