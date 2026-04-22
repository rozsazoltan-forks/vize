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
assert.doesNotMatch(
  ssrCompiled.code,
  /__vapor/,
  "SSR builds should not keep the Vapor component marker when the compiler falls back to VDOM",
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

assert.equal(
  batchResult.successCount,
  1,
  "Batch compilation should succeed for the SSR regression",
);
assert.equal(
  batchResult.failedCount,
  0,
  "Batch compilation should stay clean for the SSR regression",
);
assert.equal(batchResult.results.length, 1, "Batch compilation should return a single file result");
assert.match(
  batchResult.results[0]?.code ?? "",
  /\$setup\.Primitive/,
  "Batch SSR compilation should match single-file binding resolution for lowercase imported components",
);
assert.doesNotMatch(
  batchResult.results[0]?.code ?? "",
  /__vapor/,
  "Batch SSR compilation should also drop the Vapor marker when falling back to VDOM",
);

const antDesignSource = `<script setup lang="ts">
import { Form, Input } from "ant-design-vue";
</script>

<template>
  <Form.Item label="Teacher">
    <Input />
  </Form.Item>
</template>`;

const antDesignDomCompiled = compileFile(
  "/src/AntDesignTeacher.vue",
  new Map(),
  { sourceMap: false, ssr: false, vapor: false },
  antDesignSource,
);

assert.match(
  antDesignDomCompiled.code,
  /_create(?:Block|VNode)\(Form\.Item/,
  "DOM builds should compile dotted component tags to direct setup member expressions",
);
assert.doesNotMatch(
  antDesignDomCompiled.code,
  /_resolveComponent\("Form\.Item"\)/,
  "DOM builds must not fall back to runtime resolution for dotted imported components",
);

const antDesignSsrCompiled = compileFile(
  "/src/AntDesignTeacher.vue",
  new Map(),
  { sourceMap: false, ssr: true, vapor: false },
  antDesignSource,
);

assert.match(
  antDesignSsrCompiled.code,
  /\$setup\.Form\.Item/,
  "SSR builds should preserve setup member expressions for dotted imported components",
);
assert.doesNotMatch(
  antDesignSsrCompiled.code,
  /_resolveComponent\("Form\.Item"\)/,
  "SSR builds must not fall back to runtime resolution for dotted imported components",
);

const antDesignVaporCompiled = compileFile(
  "/src/AntDesignTeacher.vue",
  new Map(),
  { sourceMap: false, ssr: false, vapor: true },
  antDesignSource,
);

assert.match(
  antDesignVaporCompiled.code,
  /const _component_Form_Item = _ctx\.Form\.Item/,
  "Vapor builds should resolve dotted imported components through ctx member expressions",
);
assert.doesNotMatch(
  antDesignVaporCompiled.code,
  /_resolveComponent\("Form\.Item"\)/,
  "Vapor builds must not leave dotted imported components to runtime resolution",
);

const customRendererSource = `<script setup lang="ts">
import { Primitive } from "@tresjs/core";
const visible = true;
</script>

<template>
  <mesh>
    <group v-if="visible">
      <primitive />
    </group>
  </mesh>
</template>`;

const customRendererClientCompiled = compileFile(
  "/src/TresCustomRenderer.vue",
  new Map(),
  { sourceMap: false, ssr: false, vapor: true, customRenderer: true },
  customRendererSource,
);

assert.match(
  customRendererClientCompiled.code,
  /const _component_primitive = _ctx\.Primitive/,
  "Custom renderer Vapor builds should still resolve imported lowercase components through setup bindings",
);
assert.doesNotMatch(
  customRendererClientCompiled.code,
  /_resolveComponent\("(mesh|group|primitive)"\)/,
  "Custom renderer Vapor builds must not fall back to runtime component resolution for intrinsic tags",
);

const customRendererSsrCompiled = compileFile(
  "/src/TresCustomRenderer.vue",
  new Map(),
  { sourceMap: false, ssr: true, vapor: true, customRenderer: true },
  customRendererSource,
);

assert.match(
  customRendererSsrCompiled.code,
  /\$setup\.Primitive/,
  "Custom renderer SSR builds should keep imported lowercase components bound to setup",
);
assert.doesNotMatch(
  customRendererSsrCompiled.code,
  /_resolveComponent\("(mesh|group|primitive)"\)/,
  "Custom renderer SSR builds must not resolve intrinsic tags as Vue components",
);
assert.doesNotMatch(
  customRendererSsrCompiled.code,
  /<primitive><\/primitive>/,
  "Custom renderer SSR builds must not stringify imported lowercase components as plain elements",
);
assert.doesNotMatch(
  customRendererSsrCompiled.code,
  /__vapor/,
  "Custom renderer SSR builds should not keep the Vapor marker after the SSR fallback",
);

console.log("✅ vite-plugin-vize compiler tests passed!");
