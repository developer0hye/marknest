# Phase 8 Output And Operations

- Source: `toml` crate docs, https://docs.rs/toml/latest/toml/
- Source: Chrome DevTools Protocol `Page.printToPDF`, https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-printToPDF
- Source: Docker build best practices, https://docs.docker.com/develop/develop-images/dockerfile_best-practices/
- Source: `tracing-subscriber` docs, https://docs.rs/tracing-subscriber/latest/tracing_subscriber/
- Source: `tower-http` trace docs, https://docs.rs/tower-http/latest/tower_http/trace/
- Use `toml` + `serde` for simple config loading and keep precedence explicit in code: CLI > config > env > defaults.
- Resolve config-relative asset paths from the config file directory so checked-in presets stay portable.
- Use `Page.printToPDF` `headerTemplate` and `footerTemplate` instead of trying to fake page numbers in document HTML.
- Translate user tokens like `{{pageNumber}}` and `{{totalPages}}` to the protocol's template placeholders and reject scriptable template content.
- Include runtime metadata in reports even when version discovery is best-effort, so reproducing output starts from the same renderer inputs.
- Add a single Dockerfile that builds both CLI and server binaries from the same workspace and sets the browser path in one runtime image.
- Add `TraceLayer` plus `tracing-subscriber` in the fallback server so request path, status, and latency are observable without custom logging everywhere.
