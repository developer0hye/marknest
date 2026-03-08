# Project Rules

- Always communicate and work in English.
- Before starting development, check if `PRD.md` exists in the project root. If it does, read and follow the requirements defined in it throughout the development process.
- **IMPORTANT: Follow Test-Driven Development (TDD).** See the **Testing (TDD)** section below for detailed rules.
- **IMPORTANT: Read and follow `METHODOLOGY.md`** before starting any task.
- When editing `CLAUDE.md`, use the minimum words and sentences needed to convey 100% of the meaning.
- Before each commit, run auto-formatting. Follow `FORMATTING.md` first; if it has no command, use the project's existing formatter; if none exists, use the language-default formatter and record the exact command in `FORMATTING.md`.
- After completing each planned task, run tests and commit before moving to the next task. **Skip tests if the change has no impact on runtime behavior** (e.g., docs, comments, CI config). Changes to runtime config files (YAML, JSON, etc. read by code) must still trigger tests.
- **After any code change (feature addition, bug fix, refactoring, PR merge), check if `README.md` needs updating.** If project description, usage, setup, architecture, or API changed, update `README.md` with clear, concise language. Keep it minimal — only document what users need to know.

## Testing (TDD)

- Write tests first. Follow Red-Green-Refactor: (1) failing test, (2) minimal code to pass, (3) refactor.
- Use real-world scenarios and realistic data in tests. Prefer actual use cases over trivial/contrived examples.
- **Never overfit to tests.** Implementation must solve the general problem, not just the specific test cases. No hardcoded returns, no input-matching conditionals, no logic that only handles test values. Use triangulation — when a fake/hardcoded implementation passes, add tests with different inputs to force generalization.
- Test behavior, not implementation. Assert on observable outcomes, not internal details — tests must survive refactoring.
- Every new feature or bug fix must have corresponding tests.
- **Optimize test execution speed.** Run independent tests in parallel (multi-process/multi-thread, async, or parallel runner). Use the framework's built-in parallel execution (e.g., `pytest-xdist`, `jest --workers`, `go test -parallel`). Keep each test isolated — no shared mutable state — so parallel execution is safe.
- For I/O-bound tests (network, file, DB), prefer async or use mocks to avoid blocking. For CPU-bound tests, use multi-process/multi-thread parallelism.
- If full test suite exceeds 30 seconds, investigate: split slow integration tests from fast unit tests, run unit tests first for quick feedback.
- **Skip tests when no runtime impact.** In CI/CD, use path filters to trigger tests only when source code, test files, or runtime config files (YAML, JSON, etc. read by code) are modified. Non-runtime changes (docs, README, `.md`, CI pipeline config) should not trigger test runs. Locally, verify whether a change affects runtime behavior before running tests.

## Logging

- Add structured logs at key decision points, state transitions, and external calls — not every line. Logs alone should reveal the execution flow and root cause.
- Include context: request/correlation IDs, input parameters, elapsed time, and outcome (success/failure with reason).
- Use appropriate log levels: `ERROR` for failures requiring action, `WARN` for recoverable issues, `INFO` for business events, `DEBUG` for development diagnostics.
- Keep logging thread-safe. Use thread/coroutine IDs in log context for multi-threaded environments.
- Never log sensitive data (credentials, tokens, PII). Mask or omit them.
- Avoid excessive logging in hot paths — logging must not degrade performance or increase latency noticeably.

## Naming

- Names must be self-descriptive — understandable without reading surrounding code. Avoid cryptic abbreviations (`proc`, `mgr`, `tmp`).
- Prefer clarity over brevity, but don't over-pad. `user_email` > `e`, `calculate_shipping_cost` > `calc` — but no need for `calculate_the_total_shipping_cost_for_user`.
- Booleans should read as yes/no questions: `is_valid`, `has_permission`, `should_retry`.
- Functions/methods should describe the action and target: `parse_config`, `send_notification`, `validate_input`.

## Types

- Prefer explicit type annotations over type inference. Implicit types (`auto`, untyped Python, `any`) force readers to infer from context, increasing ambiguity.
- At minimum, annotate function signatures (parameters and return types). Annotate variables when the type isn't obvious from the assigned value.

## Comments

- Explain **why**, not what. Code already shows what it does — comments should capture intent, constraints, and non-obvious decisions.
- Comment business rules, workarounds, and "why this approach over the obvious one" — context that can't be inferred from code alone.
- Mark known limitations with `TODO(reason)` or `FIXME(reason)` — always include why, not just what.
- Delete comments when the code changes — outdated comments are worse than no comments.

## Reference Projects

- First check `references/INDEX.md` if it exists whenever the task includes at least one of the following:
  - adding a new feature
  - changing runtime behavior
  - changing public CLI or API behavior
  - introducing a new dependency
  - modifying code in more than one module or crate
  - making an architectural or data model decision
- Read only the single most relevant reference file unless more are clearly necessary.
- If no relevant project exists in `references/` for a task above, search for a well-maintained open-source project that solves a similar problem. Search across languages when useful. Evaluate by maintenance activity and architectural similarity.
- Add each useful new reference to `references/INDEX.md` and create a matching detail file in `references/`. Keep detail files under 50 lines.
- Skip this step for documentation-only edits, formatting-only edits, comments-only edits, renames without behavior changes, and isolated test-only changes that do not require new design decisions.
- Cite the reference project that informed the implementation when applying its patterns.

## Git Configuration

- All commits must use the local git config `user.name` and `user.email`. Verify with `git config user.name` and `git config user.email` before committing.
- All commits must include `Signed-off-by` line (always use `git commit -s`). The `Signed-off-by` name must match the commit author.

## Branching & PR Workflow

- All changes go through pull requests. No direct commits to `main`.
- Branch naming: `<type>/<short-description>` (e.g., `feat/add-parser`, `fix/table-bug`).
- One branch = one focused unit of work.
- **Use git worktrees** for all branch work. Do not use `git checkout`/`git switch` in the main repo.
  - Create: `git worktree add ../<repo-name>-<branch-name> -b <type>/<short-description>`
  - Work and push from inside the worktree.
  - Do not delete worktrees immediately after task completion — remove only when starting new work or upon user confirmation.

## PR Merge Procedure

Follow all steps in order:

1. Rewrite PR description if empty/unclear via `gh pr edit`. Include: what changed, why, key changes, and relevant context.
2. Cross-reference related issues (`gh issue list`). Use "Related: #N" — avoid auto-close keywords unless instructed.
3. Check for conflicts. If `main` has advanced, rebase/merge as needed.
4. Wait for CI to pass: `gh pr checks <number> --watch`. Abort if tests fail.
5. Final code review via `gh pr diff <number>` — check for debug statements, hardcoded paths, credentials, unused imports.
6. Merge: `gh pr merge <number> --merge`. **Never use `--delete-branch`** (worktree depends on the branch).
7. Return to main repo, `git pull` to sync.
8. Remove worktree: `git worktree remove ../<repo-name>-<branch-name>`
9. Delete local branch: `git branch -d <branch-name>`
10. Delete remote branch: `git push origin --delete <branch-name>`

## Releases

- When creating a GitHub Release, include a **Contributors** section crediting all external contributors since the previous release.
- Use `git log <prev-tag>..HEAD --format='%an' | sort -u` to find contributors. List each with their GitHub profile link and a brief summary of their contribution.
