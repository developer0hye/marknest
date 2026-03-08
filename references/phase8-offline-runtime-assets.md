# Phase 8 Offline Runtime Assets

- Source: Trunk assets guide, https://trunkrs.dev/guide/assets/index.html
- Source: MathJax v3 hosting docs, https://docs.mathjax.org/en/v3.2/web/hosting.html
- Source: Mermaid package release, https://www.npmjs.com/package/mermaid/v/11.11.0
- Source: MathJax package release, https://www.npmjs.com/package/mathjax-full/v/3.2.2
- Source: html2pdf.js docs, https://ekoopmans.github.io/html2pdf.js/
- Source: MDN `HTMLIFrameElement.srcdoc`, https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/srcdoc
- Use Trunk `rel="copy-dir"` for a pinned `runtime-assets/` directory instead of introducing an npm build step.
- Keep Mermaid and MathJax on vendored local files so Browser Fast, CLI, and fallback rendering work without external CDN access.
- `srcdoc` frames resolve relative asset URLs against the embedding document URL, so a copied `runtime-assets/` tree can serve preview/export HTML directly.
- Bundle exact distributable files in-repo and keep upstream license notices with them.
- Keep MathJax on v3 for this slice because the current runtime uses `tex2svgPromise`; pinning `3.2.2` avoids an unrelated v4 migration.
