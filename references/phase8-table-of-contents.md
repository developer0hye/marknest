# Phase 8 Table of Contents

- Use `pulldown-cmark` heading events instead of post-processing rendered HTML.
- Source: `Tag::Heading` docs on docs.rs: https://docs.rs/pulldown-cmark/latest/pulldown_cmark/enum.Tag.html
- Preserve explicit heading IDs from Markdown when they exist; only generate IDs for headings that do not already have one.
- Generate stable section links so TOC entries and manual `#section` links land on the same heading anchors.
- Keep the first implementation lightweight: build the TOC from rendered document headings and insert it near the top of the body HTML.
- Source: GitHub Docs on section links and README outlines: https://docs.github.com/github/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax
