import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { patchUnoCssBridge } from "./unocss.ts";

const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "vize-unocss-"));
const sourcePath = path.join(tempRoot, "App.vue");
const virtualId = `\0${sourcePath}.ts`;

fs.writeFileSync(
  sourcePath,
  `<template><div flex="~ col gap-2" text="sm slate-700">hello</div></template>\n`,
  "utf-8",
);

{
  let receivedCode = "";
  let receivedId = "";

  const plugins = [
    {
      name: "unocss:global:build",
      transform(code: string, id: string) {
        receivedCode = code;
        receivedId = id;
        return null;
      },
    },
  ];

  patchUnoCssBridge(plugins);
  plugins[0]!.transform!("export default {}", virtualId);

  assert.equal(receivedId, sourcePath);
  assert.match(receivedCode, /export default \{\}/);
  assert.match(receivedCode, /flex="~ col gap-2"/);
  assert.match(receivedCode, /text="sm slate-700"/);
}

{
  let receivedCode = "";
  let receivedId = "";

  const plugins = [
    {
      name: "unocss:transformers",
      transform(code: string, id: string) {
        receivedCode = code;
        receivedId = id;
        return null;
      },
    },
  ];

  patchUnoCssBridge(plugins);
  plugins[0]!.transform!("export default {}", virtualId);

  assert.equal(receivedId, sourcePath);
  assert.equal(receivedCode, "export default {}");
}

{
  let callCount = 0;

  const plugins = [
    {
      name: "unocss:global:build",
      transform() {
        callCount += 1;
        return null;
      },
    },
  ];

  patchUnoCssBridge(plugins);
  patchUnoCssBridge(plugins);
  plugins[0]!.transform!("export default {}", virtualId);

  assert.equal(callCount, 1);
}

console.log("✅ vite-plugin-vize UnoCSS bridge tests passed!");
