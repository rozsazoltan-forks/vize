import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";

import type { VizePluginState } from "./state.js";
import { resolveIdHook } from "./resolve.js";
import { toVirtualId } from "../virtual.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const workspaceRoot = path.resolve(__dirname, "../../../..");

function createState(root: string): VizePluginState {
  return {
    cache: new Map(),
    collectedCss: new Map(),
    precompileMetadata: new Map(),
    pendingHmrUpdateTypes: new Map(),
    isProduction: false,
    root,
    clientViteBase: "/",
    serverViteBase: "/",
    server: {} as never,
    filter: () => true,
    scanPatterns: ["**/*.vue"],
    ignorePatterns: [],
    mergedOptions: {},
    initialized: true,
    dynamicImportAliasRules: [],
    cssAliasRules: [],
    extractCss: false,
    clientViteDefine: {},
    serverViteDefine: {},
    logger: {
      log() {},
      info() {},
      warn() {},
      error() {},
    } as never,
  };
}

const nullResolveContext = {
  resolve: async () => null,
};

function expectResolvedId(resolved: Awaited<ReturnType<typeof resolveIdHook>>): string {
  assert.equal(typeof resolved, "string");
  return resolved;
}

{
  const projectRoot = path.join(workspaceRoot, "tests", "_fixtures", "_git", "npmx.dev");
  const importer = toVirtualId(path.join(projectRoot, "app", "pages", "index.vue"));
  const resolved = await resolveIdHook(
    nullResolveContext,
    createState(projectRoot),
    "vue-data-ui/style.css",
    importer,
  );

  assert.match(expectResolvedId(resolved), /vue-data-ui\/dist\/style\.css$/);
}

{
  const projectRoot = path.join(workspaceRoot, "tests", "_fixtures", "_git", "vuefes-2025");
  const importer = toVirtualId(path.join(projectRoot, "app", "pages", "index.vue"));
  const resolved = await resolveIdHook(
    nullResolveContext,
    createState(projectRoot),
    "@primevue/forms/resolvers/valibot?nuxt_component=async",
    importer,
  );

  assert.match(
    expectResolvedId(resolved),
    /@primevue\/forms\/resolvers\/valibot\/index\.mjs\?nuxt_component=async$/,
  );
}

console.log("✅ vite-plugin-vize resolve tests passed!");
