import test from "node:test";
import assert from "node:assert/strict";

import {
  buildSourceSnapshot,
  calculateHeadingCoverage,
  calculateTokenCoverage,
} from "./lib/text_metrics.mjs";

test("buildSourceSnapshot preserves underscore-bearing identifiers and flags", () => {
  const snapshot = buildSourceSnapshot(`
# Equatiomatic

Run extract_eq(mod1) or preview_eq(mod1).
Use terms_per_line, operator_location, ital_vars, use_coefs, and fix_signs.
Set TORCH_DEVICE=cuda and pass --json_path results.json.
`);

  for (const token of [
    "extract_eq",
    "preview_eq",
    "terms_per_line",
    "operator_location",
    "ital_vars",
    "use_coefs",
    "fix_signs",
    "torch_device",
    "json_path",
  ]) {
    assert.equal(snapshot.sourceTokens.includes(token), true, token);
  }

  for (const token of [
    "extracteq",
    "previeweq",
    "termsperline",
    "operatorlocation",
    "italvars",
    "usecoefs",
    "fixsigns",
    "torchdevice",
    "jsonpath",
  ]) {
    assert.equal(snapshot.sourceTokens.includes(token), false, token);
  }
});

test("calculateTokenCoverage matches preserved underscore-bearing identifiers", () => {
  const snapshot = buildSourceSnapshot(`
Run extract_eq(mod1) or preview_eq(mod1).
Use terms_per_line, operator_location, ital_vars, use_coefs, and fix_signs.
Set TORCH_DEVICE=cuda and pass --json_path results.json.
`);

  const coverage = calculateTokenCoverage(
    snapshot,
    "Run extract_eq(mod1) or preview_eq(mod1). Use terms_per_line, operator_location, ital_vars, use_coefs, and fix_signs. Set TORCH_DEVICE=cuda and pass --json_path results.json.",
  );

  assert.equal(coverage.coverage, 1);
  assert.deepEqual(coverage.missingTokens, []);
});

test("calculateHeadingCoverage matches LaTeX-styled headings against extracted math glyphs", () => {
  const snapshot = buildSourceSnapshot(`
### 10. Orthonormal sets in $\\mathbb{R}^2$ and $\\mathbb{R}^3$
### 55. The Multiplicative Property of Determinants: $\\det(AB) = \\det(A)\\det(B)$
`);

  const coverage = calculateHeadingCoverage(
    snapshot,
    "10. Orthonormal sets in 𝑅2 and 𝑅3\n55. The Multiplicative Property of Determinants: det (𝐴𝐵) = det (𝐴) det (𝐵)",
  );

  assert.equal(coverage.coverage, 1);
  assert.deepEqual(coverage.missingHeadings, []);
});

test("buildSourceSnapshot excludes LaTeX math spans from source tokens", () => {
  const snapshot = buildSourceSnapshot(`
Use $\\mathbf{x}_i = \\frac{1}{2}$ for the state.

$$
\\sum_{i=1}^{n} x_i
$$

Run extract_eq outside math.
`);

  assert.equal(snapshot.sourceTokens.includes("use"), true);
  assert.equal(snapshot.sourceTokens.includes("for"), true);
  assert.equal(snapshot.sourceTokens.includes("state"), true);
  assert.equal(snapshot.sourceTokens.includes("run"), true);
  assert.equal(snapshot.sourceTokens.includes("extract_eq"), true);
  assert.equal(snapshot.sourceTokens.includes("outside"), true);
  assert.equal(snapshot.sourceTokens.includes("math"), true);
  assert.equal(snapshot.sourceTokens.includes("mathbf"), false);
  assert.equal(snapshot.sourceTokens.includes("frac"), false);
  assert.equal(snapshot.sourceTokens.includes("x_i"), false);
  assert.equal(snapshot.sourceTokens.includes("sum_"), false);
});

test("buildSourceSnapshot keeps literal delimiter prose while stripping later math spans", () => {
  const snapshot = buildSourceSnapshot(`
MathJax ($$ and $ are delimiters).

**Detected Text** The potential $V_ i$ of cell $\\mathcal{C}_ i$ centred at position $\\mathbf{r}_ i$ is related to the surface charge densities $\\sigma_ j$ through the superposition principle as: $$V_ i = \\sum_ {j=0}^{N} \\frac{\\sigma_ j}{4\\pi\\varepsilon_ 0} \\int_ {\\mathcal{C}_ j} \\mathrm{d}^2\\mathbf{r}'$$ where the integral is evaluated over the cell.
`);

  assert.equal(snapshot.sourceTokens.includes("mathjax"), true);
  assert.equal(snapshot.sourceTokens.includes("are"), true);
  assert.equal(snapshot.sourceTokens.includes("delimiters"), true);
  assert.equal(snapshot.sourceTokens.includes("detected"), true);
  assert.equal(snapshot.sourceTokens.includes("text"), true);
  assert.equal(snapshot.sourceTokens.includes("potential"), true);
  assert.equal(snapshot.sourceTokens.includes("where"), true);
  assert.equal(snapshot.sourceTokens.includes("integral"), true);
  assert.equal(snapshot.sourceTokens.includes("sum_"), false);
  assert.equal(snapshot.sourceTokens.includes("frac"), false);
  assert.equal(snapshot.sourceTokens.includes("sigma_"), false);
  assert.equal(snapshot.sourceTokens.includes("varepsilon_"), false);
  assert.equal(snapshot.sourceTokens.includes("int_"), false);
  assert.equal(snapshot.sourceTokens.includes("mathcal"), false);
  assert.equal(snapshot.sourceTokens.includes("mathbf"), false);
  assert.equal(snapshot.sourceTokens.includes("mathrm"), false);
});
