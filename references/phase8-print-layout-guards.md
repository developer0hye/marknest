# Phase 8 Print Layout Guards

- Source: MDN `break-before`, https://developer.mozilla.org/en-US/docs/Web/CSS/break-before
- Source: MDN `break-inside`, https://developer.mozilla.org/en-US/docs/Web/CSS/break-inside
- Source: MDN `page-break-inside`, https://developer.mozilla.org/en-US/docs/Web/CSS/page-break-inside
- `break-before: page` is the modern way to force section starts onto a new printed page, and `page-break-before: always` remains useful as the legacy alias for compatibility.
- `break-inside: avoid` is the modern way to keep tables, figures, and code blocks together in paged output, with `page-break-inside: avoid` kept as a compatibility alias.
- Keep these rules inside `@media print` so browser preview remains readable while the PDF path gets stronger layout stability.
