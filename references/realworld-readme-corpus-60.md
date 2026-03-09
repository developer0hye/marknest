# Real-World README Corpus 60

- Source: GitHub Topics for current discovery, https://github.com/topics/
- Source: GitHub Topics `machine-learning`, https://github.com/topics/machine-learning
- Source: GitHub Topics `rust`, https://github.com/topics/rust
- Source: GitHub Topics `javascript`, https://github.com/topics/javascript
- Source: GitHub Topics `devops`, https://github.com/topics/devops
- Source: GitHub Topics `frontend`, https://github.com/topics/frontend
- Source: GitHub Topics `documentation`, https://github.com/topics/documentation
- Expand the README validation corpus from a 5-repo smoke set to a 60-entry balanced set.
- Keep the corpus Markdown-only; replace repos whose primary root README is not Markdown.
- Optimize for README pattern diversity, not raw star ranking alone.
- Cover badges, inline image rows, relative and remote images, screenshots, animated media, raw HTML, tables, long code blocks, long technical prose, Mermaid, and math-like content.
- Keep a 10-repo smoke subset for fast iteration and a 60-entry extended corpus for stricter validation.
- Reserve 10 entries for math-heavy READMEs so inline and display LaTeX patterns stay represented in the committed corpus.
- Prefer root README-focused collection so the corpus stays stable and comparable across runs.
- Use shallow or archive-based fetches later instead of full-history clones because some selected repos are very large.
- Validation goal is content preservation in print-safe PDF output, not pixel-perfect browser parity.
- If browser UI patterns do not map to PDF, prefer wrapping, stacking, expansion, or pagination over truncation or omission.
