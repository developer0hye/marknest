# Phase 8 GitHub Emoji Shortcodes

- Source: GitHub Docs, basic writing and formatting syntax, https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax
- Source: `emojis` crate docs, https://docs.rs/emojis/latest/emojis/
- GitHub documents `:EMOJICODE:` as a Markdown writing feature, so README fidelity requires shortcode rendering in normal prose.
- Code spans and fenced code blocks should remain literal; shortcode replacement belongs in Markdown text events, not as a blind pre-parse string rewrite.
- Use a maintained shortcode lookup table instead of a small custom map so common GitHub aliases like `:trophy:`, `:+1:`, and `:shipit:` stay broad and updatable.
