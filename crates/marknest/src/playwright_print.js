const fs = require("fs");
const path = require("path");
const { createRequire } = require("module");
const { pathToFileURL } = require("url");

const [
  browserPath,
  inputHtmlPath,
  outputPdfPath,
  optionsPath,
  runtimeDir,
] = process.argv.slice(2);

if (!browserPath || !inputHtmlPath || !outputPdfPath || !optionsPath || !runtimeDir) {
  console.error(
    "Usage: playwright_print.js <browser> <input-html> <output-pdf> <print-options.json> <runtime-dir>",
  );
  process.exit(1);
}

const requireFromRuntime = createRequire(path.join(path.resolve(runtimeDir), "package.json"));
const { chromium } = requireFromRuntime("playwright");

async function main() {
  const printOptions = JSON.parse(fs.readFileSync(optionsPath, "utf8"));
  const browser = await chromium.launch({
    executablePath: browserPath,
    headless: true,
    args: ["--no-sandbox", "--disable-gpu"],
  });

  try {
    const page = await browser.newPage();
    await page.goto(pathToFileURL(inputHtmlPath).href, {
      waitUntil: "domcontentloaded",
      timeout: 30000,
    });

    const renderStatus = await waitForRenderStatus(page, 15000);
    await waitForPageAssets(page, 5000);
    if (renderStatus.errors.length > 0) {
      console.log(
        JSON.stringify({
          kind: "validation",
          warnings: renderStatus.warnings,
          errors: renderStatus.errors,
          message: renderStatus.errors.join("; "),
        }),
      );
      process.exitCode = 2;
      return;
    }

    await page.pdf({
      path: outputPdfPath,
      format: printOptions.pageFormat,
      printBackground: true,
      landscape: Boolean(printOptions.landscape),
      displayHeaderFooter: Boolean(
        printOptions.headerTemplate || printOptions.footerTemplate,
      ),
      headerTemplate: printOptions.headerTemplate || "",
      footerTemplate: printOptions.footerTemplate || "",
      margin: {
        top: `${Number(printOptions.marginTopMm)}mm`,
        right: `${Number(printOptions.marginRightMm)}mm`,
        bottom: `${Number(printOptions.marginBottomMm)}mm`,
        left: `${Number(printOptions.marginLeftMm)}mm`,
      },
    });
    console.log(
      JSON.stringify({
        kind: "ok",
        warnings: renderStatus.warnings,
        errors: [],
        message: null,
      }),
    );
  } catch (error) {
    console.error(error && error.stack ? error.stack : String(error));
    process.exitCode = 1;
  } finally {
    await browser.close();
  }
}

async function waitForRenderStatus(page, timeoutMs) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    const status = await page.evaluate(() => window.__MARKNEST_RENDER_STATUS__ || null);
    if (!status) {
      return { ready: true, warnings: [], errors: [] };
    }

    if (status.ready) {
      return {
        ready: true,
        warnings: Array.isArray(status.warnings) ? status.warnings : [],
        errors: Array.isArray(status.errors) ? status.errors : [],
      };
    }

    await page.waitForTimeout(50);
  }

  throw new Error("Timed out while waiting for Mermaid and Math rendering to finish.");
}

async function waitForPageAssets(page, timeoutMs) {
  await page.evaluate(async (assetTimeoutMs) => {
    const waitForFonts = typeof document.fonts?.ready?.then === "function"
      ? document.fonts.ready.catch(() => undefined)
      : Promise.resolve();

    const waitForImages = Promise.all(
      Array.from(document.images, (image) => {
        if (image.complete) {
          return Promise.resolve();
        }

        return new Promise((resolve) => {
          const finish = () => {
            image.removeEventListener("load", finish);
            image.removeEventListener("error", finish);
            resolve();
          };

          image.addEventListener("load", finish, { once: true });
          image.addEventListener("error", finish, { once: true });
        });
      }),
    );

    await Promise.race([
      Promise.all([waitForFonts, waitForImages]),
      new Promise((resolve) => setTimeout(resolve, assetTimeoutMs)),
    ]);
  }, timeoutMs);
}

main();
