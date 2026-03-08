# Phase 8 Render Timeouts

- Source: MDN `Promise.race()`, https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race
- Source: MDN `setTimeout()`, https://developer.mozilla.org/en-US/docs/Web/API/Window/setTimeout
- Use `Promise.race()` with a timeout promise to bound per-diagram or per-expression render work in the browser runtime script.
- Keep timeout values in the shared render options model so CLI, WASM, and fallback server stay aligned instead of hardcoding different waits.
- Apply the timeout per Mermaid diagram and per Math expression, matching the PRD language more closely than a single whole-document wait.
