# Phase 8 Browser Runtime Sync

- Source: MDN `HTMLIFrameElement.contentWindow`, https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/contentWindow
- Source: MDN `HTMLIFrameElement.srcdoc`, https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/srcdoc
- Source: `html2pdf.js` README, https://github.com/eKoopmans/html2pdf.js
- `srcdoc` iframes are same-origin with the parent document unless sandboxing changes that, so the parent page can poll `iframe.contentWindow` directly.
- The browser app already renders export HTML into a hidden `srcdoc` iframe before handing it to `html2pdf.js`, so waiting on runtime status in that iframe does not require a second rendering path.
- Reuse the existing `window.__MARKNEST_RENDER_STATUS__` contract from core HTML runtime scripts instead of inventing a second browser-only signal.
- Treat missing render status as “no async runtime work” so plain Markdown documents do not pay an extra penalty.
- Keep browser timeout policy aligned with the Chromium fallback path so `auto` and `on` semantics do not drift by environment.
