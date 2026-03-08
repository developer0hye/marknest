# Phase 8 Code Block Wrapping

- Source: MDN `white-space`, https://developer.mozilla.org/en-US/docs/Web/CSS/white-space
- Source: MDN `overflow-wrap`, https://developer.mozilla.org/en-US/docs/Web/CSS/overflow-wrap
- Source: GitHub Markdown CSS, https://github.com/sindresorhus/github-markdown-css
- PDF output cannot rely on horizontal scrolling inside `pre`; long lines need a print-safe wrapping rule in the shared HTML stylesheet.
- Use `white-space: pre-wrap` to preserve indentation and line breaks while still allowing wrapping inside code blocks.
- Add `overflow-wrap: anywhere` so long unbroken tokens such as URLs, hashes, and minified strings can still wrap instead of being clipped.
- Keep the fix in shared HTML/CSS so browser preview, browser export, CLI, and fallback server all render code blocks consistently.
