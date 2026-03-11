import assert from "node:assert";
import { buildNuxtDevAssetBase } from "./utils.ts";

assert.strictEqual(
  buildNuxtDevAssetBase("/", "/_nuxt/"),
  "/_nuxt/",
  "default Nuxt dev assets should stay under /_nuxt/",
);

assert.strictEqual(
  buildNuxtDevAssetBase("/2025/", "/_nuxt/"),
  "/2025/_nuxt/",
  "Nuxt baseURL should prefix buildAssetsDir",
);

assert.strictEqual(
  buildNuxtDevAssetBase("/preview", "_assets"),
  "/preview/_assets/",
  "missing slashes should be normalized",
);

console.log("✅ nuxt utils tests passed!");
