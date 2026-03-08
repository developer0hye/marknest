import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

test("index.html wires vendored runtime assets instead of CDN browser scripts", () => {
  const indexPath = path.join(process.cwd(), "index.html");
  const html = fs.readFileSync(indexPath, "utf8");

  assert.match(html, /<base\s+data-trunk-public-url\s*\/?>/i);
  assert.match(html, /rel="copy-dir"\s+href="runtime-assets"/i);
  assert.match(html, /src="\.\/runtime-assets\/html2pdf\/html2pdf\.bundle\.min\.js"/i);
  assert.doesNotMatch(html, /cdnjs\.cloudflare\.com\/ajax\/libs\/html2pdf\.js/i);
});
