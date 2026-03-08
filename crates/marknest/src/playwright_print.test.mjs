import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs";
import fsPromises from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createRequire } from "node:module";

const SCRIPT_PATH = path.resolve("crates/marknest/src/playwright_print.js");
const RUNTIME_DIR = path.resolve("crates/marknest/playwright-runtime");
const requireFromRuntime = createRequire(path.join(RUNTIME_DIR, "package.json"));

function resolveBrowserPath() {
  const configuredPath = process.env.MARKNEST_BROWSER_PATH;
  if (configuredPath && fs.existsSync(configuredPath)) {
    return configuredPath;
  }

  try {
    const { chromium } = requireFromRuntime("playwright");
    const executablePath = chromium.executablePath();
    if (executablePath && fs.existsSync(executablePath)) {
      return executablePath;
    }
  } catch {
    return null;
  }

  return null;
}

test(
  "playwright_print tolerates hanging remote images without timing out page navigation",
  { timeout: 15000 },
  async (t) => {
    const browserPath = resolveBrowserPath();
    if (!browserPath) {
      t.skip("Playwright browser is unavailable in this environment.");
      return;
    }

    const tempDirectory = await fsPromises.mkdtemp(
      path.join(os.tmpdir(), "marknest-playwright-print-test-"),
    );
    const optionsPath = path.join(tempDirectory, "print-options.json");
    const htmlPath = path.join(tempDirectory, "document.html");
    const outputPath = path.join(tempDirectory, "output.pdf");

    const server = http.createServer((request, response) => {
      if (request.url === "/hang.png") {
        response.writeHead(200, { "Content-Type": "image/png" });
        return;
      }

      response.writeHead(404, { "Content-Type": "text/plain" });
      response.end("not found");
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    assert.ok(address && typeof address === "object");
    const hangingImageUrl = `http://127.0.0.1:${address.port}/hang.png`;

    t.after(async () => {
      await new Promise((resolve) => server.close(resolve));
      await fsPromises.rm(tempDirectory, { recursive: true, force: true });
    });

    await fsPromises.writeFile(
      optionsPath,
      `${JSON.stringify({
        pageFormat: "A4",
        landscape: false,
        headerTemplate: "",
        footerTemplate: "",
        marginTopMm: 12,
        marginRightMm: 12,
        marginBottomMm: 12,
        marginLeftMm: 12,
      })}\n`,
    );
    await fsPromises.writeFile(
      htmlPath,
      `<!doctype html>
<html>
  <body>
    <img alt="slow" src="${hangingImageUrl}">
    <p>print-safe content</p>
    <script>
      const status = { ready: false, warnings: [], errors: [] };
      window.__MARKNEST_RENDER_STATUS__ = status;
      const finalize = () => { status.ready = true; };
      if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", finalize, { once: true });
      } else {
        finalize();
      }
    </script>
  </body>
</html>
`,
    );

    const child = spawn(
      process.execPath,
      [SCRIPT_PATH, browserPath, htmlPath, outputPath, optionsPath, RUNTIME_DIR],
      {
        cwd: process.cwd(),
        env: process.env,
        stdio: ["ignore", "pipe", "pipe"],
      },
    );

    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr += String(chunk);
    });

    const exitCode = await new Promise((resolve, reject) => {
      child.on("error", reject);
      child.on("close", resolve);
    });

    assert.equal(exitCode, 0, stderr || stdout);
    const outputStat = await fsPromises.stat(outputPath);
    assert.ok(outputStat.size > 0);
  },
);
