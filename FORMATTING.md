# Formatting Rules

- This file is the single source of truth for formatting.
- Before each commit, run formatting in this order:
  1. `Project Formatter Commands` in this file.
  2. Existing project formatter if commands are empty (`Makefile`, npm/pnpm/yarn scripts, `pyproject.toml`, tool config files, etc.).
  3. `Fallback by Language` below.
- If step 2 or 3 was used, update `Project Formatter Commands` with the exact command.

## Project Formatter Commands

- Primary command: `cargo fmt --all`

## Fallback by Language

- Python: `black .`
- JavaScript/TypeScript: `prettier --write .`
- Go: `gofmt -w` (changed files or target dirs)
- Rust: `cargo fmt --all`
- Java: `google-java-format -i`
- Kotlin: `ktlint -F`
- C/C++: `clang-format -i`
- Shell: `shfmt -w`
- Terraform: `terraform fmt -recursive`
- YAML/JSON/Markdown: `prettier --write .`
