# Phase 8 HTML Sanitization

- Use `ammonia` as the HTML fragment sanitizer instead of ad hoc tag stripping.
- Source: `ammonia::Builder` docs on docs.rs: https://docs.rs/ammonia/latest/ammonia/struct.Builder.html
- Keep sanitization scoped to rendered document body HTML; do not sanitize MarkNest's own runtime wrapper or print script.
- Default `sanitize_html` to `true`, with an explicit opt-out only for trusted documents.
- Preserve Markdown-generated structure by allowing safe presentation attributes such as `class`, `id`, `title`, `width`, `height`, `align`, `checked`, `disabled`, `open`, and `type`.
- Preserve common README/raw-HTML tags needed in practice: `details`, `summary`, `figure`, `figcaption`, and `input`.
- Keep relative URLs allowed; block scripts, event-handler attributes, `javascript:` URLs, and unsupported embeds such as `<iframe>`.
- Apply sanitization before local/remote image materialization so safe `img src` values can still be inlined afterward.
