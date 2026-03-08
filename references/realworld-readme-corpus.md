# Real-World README Corpus

- Source: GitHub Markdown docs for Mermaid, https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-diagrams
- Source: GitHub Markdown docs for math, https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/writing-mathematical-expressions
- Source: `mermaid-js/mermaid` README, https://github.com/mermaid-js/mermaid
- Source: `typst/typst` README, https://github.com/typst/typst
- Source: `ultralytics/ultralytics` README, https://github.com/ultralytics/ultralytics
- Source: `facebookresearch/segment-anything` README, https://github.com/facebookresearch/segment-anything
- Source: `pytorch/pytorch` README, https://github.com/pytorch/pytorch
- Use a small corpus with clearly different README shapes instead of many near-duplicates.
- `mermaid-js/mermaid` is the Mermaid-heavy case with repeated fenced diagrams and some relative image assets.
- `typst/typst` is the math case because the README includes inline `$...$` expressions and formula-oriented content.
- `ultralytics/ultralytics` stresses badge-heavy HTML, dense tables, and many remote images.
- `facebookresearch/segment-anything` stresses relative image assets referenced from the README.
- `pytorch/pytorch` adds a large, popular README with tables plus many remote `img` tags.
- For evaluation, shallow sparse checkouts or archive snapshots are sufficient; we only need the README and any locally referenced assets, not full history.
