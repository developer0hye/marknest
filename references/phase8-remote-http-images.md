# Phase 8 Remote HTTP Images

- Source: GitHub Docs, relative links and image paths in Markdown files, https://docs.github.com/en/repositories/working-with-files/using-files/relative-links-in-readmes
- Source: GitHub Docs, basic writing and formatting syntax, https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax
- Source: MDN Fetch API, https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API
- Source: MDN CORS guide, https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/CORS
- Source: MDN `load` event, https://developer.mozilla.org/en-US/docs/Web/API/HTMLElement/load_event
- Source: MDN `error` event, https://developer.mozilla.org/en-US/docs/Web/API/HTMLElement/error_event
- GitHub README images commonly appear as repository-relative links and as `github.com/.../blob/...` URLs with `?raw=true`, so normalizing common GitHub wrappers is a compatibility fix, not a special case.
- GitHub README image paths that start with `/` are repository-root-relative, not host filesystem absolute paths, so local asset resolution should normalize them against the extracted repo root.
- Keep core analysis synchronous and deterministic: record normalized fetch metadata during analysis, and defer network work to runtime-specific materialization steps.
- For native CLI/server flows, a small blocking HTTP client is enough because the render path is already blocking and bounded by per-request timeouts.
- For browser flows, remote fetch can fail because of CORS even when plain `<img>` display might work, so browser export needs explicit fallback policy instead of assuming fetch parity with native.
- Treat remote fetch failure as a warning by default, keep original URLs when materialization fails, and let stricter workflows elevate warnings to failures.
- Materialized debug artifacts should use rewritten HTML so successful remote images become reproducible offline.
