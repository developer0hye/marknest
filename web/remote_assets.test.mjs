import test from "node:test";
import assert from "node:assert/strict";

import {
  hasFailedRemoteAssets,
  materializeRemoteImages,
} from "./remote_assets.mjs";

test("materializeRemoteImages rewrites successful remote fetches into data URIs", async () => {
  const remoteUrl = "https://example.com/assets/diagram.png";
  const result = await materializeRemoteImages({
    html: `<html><body><img src="${remoteUrl}" alt="Remote"></body></html>`,
    assets: [
      {
        entry_path: "README.md",
        original_reference: remoteUrl,
        fetch_url: remoteUrl,
      },
    ],
    fetchImpl: async (url) => {
      assert.equal(url, remoteUrl);
      return new Response(new Uint8Array([137, 80, 78, 71]), {
        status: 200,
        headers: { "content-type": "image/png" },
      });
    },
  });

  assert.match(result.html, /data:image\/png;base64,/);
  assert.equal(result.remoteAssets[0].status, "inlined");
  assert.deepEqual(result.warnings, []);
  assert.equal(hasFailedRemoteAssets(result.remoteAssets), false);
});

test("materializeRemoteImages records failed remote fetches as warnings", async () => {
  const remoteUrl = "https://example.com/assets/blocked.png";
  const result = await materializeRemoteImages({
    html: `<html><body><img src="${remoteUrl}" alt="Remote"></body></html>`,
    assets: [
      {
        entry_path: "README.md",
        original_reference: remoteUrl,
        fetch_url: remoteUrl,
      },
    ],
    fetchImpl: async () => {
      throw new TypeError("Failed to fetch");
    },
    timeoutMs: 20,
  });

  assert.equal(result.html.includes(remoteUrl), true);
  assert.equal(result.remoteAssets[0].status, "failed");
  assert.match(result.remoteAssets[0].message, /Failed to fetch/);
  assert.equal(result.warnings.length, 1);
  assert.equal(hasFailedRemoteAssets(result.remoteAssets), true);
});
