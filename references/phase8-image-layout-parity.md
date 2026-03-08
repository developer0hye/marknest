# Phase 8 Image Layout Parity

- Source: MDN `<img>`, https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img
- Source: MDN `height`, https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/height
- Source: `github-markdown-css`, https://github.com/sindresorhus/github-markdown-css
- Badge-heavy READMEs rely on images staying inline inside paragraphs, so the shared stylesheet should not force all `img` elements to `display: block`.
- Explicit HTML image dimensions such as `height="150"` must remain effective; a blanket `height: auto` author rule overrides those presentational hints.
- Keep `img` responsive with `max-width: 100%`, then add narrow selectors for standalone block images instead of treating every image as a block element.
