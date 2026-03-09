# README Corpus Validator

- Source: GitHub REST docs `Get a repository`, https://docs.github.com/en/rest/repos/repos?apiVersion=2022-11-28#get-a-repository
- Source: GitHub REST docs `Get a repository README`, https://docs.github.com/en/rest/repos/contents?apiVersion=2022-11-28#get-a-repository-readme
- Source: GitHub REST docs `Download a repository archive (tar)`, https://docs.github.com/en/rest/repos/contents?apiVersion=2022-11-28#download-a-repository-archive-tar
- Source: `pixelmatch` README, https://github.com/mapbox/pixelmatch
- Source: `pngjs` README, https://github.com/pngjs/pngjs
- Use pinned GitHub metadata plus archive snapshots instead of live default-branch clones so the 60-entry corpus baselines stay reproducible.
- Compare PDFs with a hybrid gate: text coverage and heading presence are blocking; pixel diffs are advisory unless paired with obvious content loss signals.
- Use `pdftotext` and `pdftoppm` as the portable extraction layer so the validator can check missing text, blank pages, and per-page PNG diffs.
