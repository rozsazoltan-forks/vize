/**
 * High-performance native Vite plugin for Vue SFC compilation powered by Vize.
 */

export { vize } from "./plugin/index.js";
export { defineConfig, loadConfig, vizeConfigStore } from "./config.js";
export { rewriteStaticAssetUrls as __internal_rewriteStaticAssetUrls } from "./transform.js";
export type { VizeOptions, CompiledModule, VizeConfig, LoadConfigOptions } from "./types.js";

// Test-only export for snapshot coverage (re-exported for backward compat).
import { rewriteStaticAssetUrls } from "./transform.js";
export const __internal = {
  rewriteStaticAssetUrls,
};

import { vize } from "./plugin/index.js";
export default vize;
