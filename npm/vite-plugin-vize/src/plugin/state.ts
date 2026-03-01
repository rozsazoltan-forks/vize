/**
 * Plugin state type and batch compilation logic.
 */

import type { ViteDevServer } from "vite";
import fs from "node:fs";
import { glob } from "tinyglobby";

import type { VizeOptions, CompiledModule } from "../types.js";
import { compileBatch } from "../compiler.js";
import { resolveCssImports, type CssAliasRule } from "../utils/css.js";
import { type DynamicImportAliasRule } from "../virtual.js";
import { createLogger } from "../transform.js";

export interface VizePluginState {
  cache: Map<string, CompiledModule>;
  collectedCss: Map<string, string>;
  isProduction: boolean;
  root: string;
  clientViteBase: string;
  serverViteBase: string;
  server: ViteDevServer | null;
  filter: (id: string) => boolean;
  scanPatterns: string[] | null;
  ignorePatterns: string[];
  mergedOptions: VizeOptions;
  initialized: boolean;
  dynamicImportAliasRules: DynamicImportAliasRule[];
  cssAliasRules: CssAliasRule[];
  extractCss: boolean;
  clientViteDefine: Record<string, string>;
  serverViteDefine: Record<string, string>;
  logger: ReturnType<typeof createLogger>;
}

/**
 * Pre-compile all Vue files matching scan patterns.
 */
export async function compileAll(state: VizePluginState): Promise<void> {
  const startTime = performance.now();
  const files = await glob(state.scanPatterns!, {
    cwd: state.root,
    ignore: state.ignorePatterns,
    absolute: true,
  });

  state.logger.info(`Pre-compiling ${files.length} Vue files...`);

  // Read all files
  const fileContents: { path: string; source: string }[] = [];
  for (const file of files) {
    try {
      const source = fs.readFileSync(file, "utf-8");
      fileContents.push({ path: file, source });
    } catch (e) {
      state.logger.error(`Failed to read ${file}:`, e);
    }
  }

  // Batch compile using native parallel processing
  const result = compileBatch(fileContents, state.cache, {
    ssr: state.mergedOptions.ssr ?? false,
  });

  // Collect CSS for production extraction.
  // Skip files with delegated styles (preprocessor/CSS Modules) -- those go through
  // Vite's CSS pipeline and are extracted by Vite itself.
  if (state.isProduction) {
    for (const fileResult of result.results) {
      if (fileResult.css) {
        const cached = state.cache.get(fileResult.path);
        const hasDelegated = cached?.styles?.some(
          (s) =>
            (s.lang !== null && ["scss", "sass", "less", "stylus", "styl"].includes(s.lang)) ||
            s.module !== false,
        );
        if (!hasDelegated) {
          state.collectedCss.set(
            fileResult.path,
            resolveCssImports(fileResult.css, fileResult.path, state.cssAliasRules, false),
          );
        }
      }
    }
  }

  const elapsed = (performance.now() - startTime).toFixed(2);
  state.logger.info(
    `Pre-compilation complete: ${result.successCount} succeeded, ${result.failedCount} failed (${elapsed}ms, native batch: ${result.timeMs.toFixed(2)}ms)`,
  );
}
