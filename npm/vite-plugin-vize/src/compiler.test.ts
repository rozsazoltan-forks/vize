import assert from "node:assert/strict";

import { compileBatch, compileFile } from "./compiler.ts";

const tresSource = `<script setup lang="ts">
import { Primitive } from "@tresjs/core";
const msg = "hello";
</script>

<template>
  <primitive />
  <div>{{ msg }}</div>
</template>`;

const clientCompiled = compileFile(
  "/src/TresPrimitive.vue",
  new Map(),
  { sourceMap: false, ssr: false, vapor: true },
  tresSource,
);

assert.match(
  clientCompiled.code,
  /const _component_primitive = _ctx\.Primitive/,
  "Client Vapor builds should resolve lowercase imported components through setup bindings",
);
assert.match(
  clientCompiled.code,
  /const __vaporRender = render/,
  "Client Vapor builds should preserve the render alias used by script setup output",
);

const ssrCompiled = compileFile(
  "/src/TresPrimitive.vue",
  new Map(),
  { sourceMap: false, ssr: true, vapor: true },
  tresSource,
);

assert.match(
  ssrCompiled.code,
  /\$setup\.Primitive/,
  "SSR builds should resolve lowercase imported components from setup bindings",
);
assert.doesNotMatch(
  ssrCompiled.code,
  /_resolveComponent\("primitive"\)/,
  "SSR builds must not fall back to runtime component resolution for imported lowercase components",
);
assert.match(
  ssrCompiled.code,
  /ssrRender/,
  "SSR builds should still emit server-renderer code paths when Vapor is enabled for the client",
);

const batchResult = compileBatch(
  [
    {
      path: "/src/TresPrimitive.vue",
      source: tresSource,
    },
  ],
  new Map(),
  { ssr: true, vapor: true },
);

assert.equal(batchResult.successCount, 1, "Batch compilation should succeed for the SSR regression");
assert.equal(batchResult.failedCount, 0, "Batch compilation should stay clean for the SSR regression");
assert.equal(batchResult.results.length, 1, "Batch compilation should return a single file result");
assert.match(
  batchResult.results[0]?.code ?? "",
  /\$setup\.Primitive/,
  "Batch SSR compilation should match single-file binding resolution for lowercase imported components",
);

console.log("✅ vite-plugin-vize compiler tests passed!");
