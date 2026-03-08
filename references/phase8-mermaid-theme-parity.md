# Phase 8 Mermaid Theme Parity

- Source: Mermaid theme configuration docs, https://mermaid.js.org/config/theming.html
- Source: Mermaid config schema docs, https://mermaid.js.org/config/schema-docs/config
- Mermaid documents `default` as the normal site theme and `neutral` as the black-and-white print theme.
- For README fidelity, the shared runtime should not hardcode `neutral` for all diagrams because that strips the color semantics users expect from Mermaid examples.
- Keep Mermaid theme selection explicit in the runtime config so future preset-specific mapping stays testable instead of hiding a magic string inside the browser script.
