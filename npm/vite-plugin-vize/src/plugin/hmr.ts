import type { HmrContext } from "vite";
import path from "node:path";

import type { VizePluginState } from "./state.js";
import { compileFile } from "../compiler.js";
import { detectHmrUpdateType, type HmrUpdateType } from "../hmr.js";
import { toVirtualId } from "../virtual.js";
import { resolveCssImports } from "../utils/css.js";

export async function handleHotUpdateHook(
  state: VizePluginState,
  ctx: HmrContext,
): Promise<import("vite").ModuleNode[] | void> {
  const { file, server, read } = ctx;

  if (file.endsWith(".vue") && state.filter(file)) {
    try {
      const source = await read();

      const prevCompiled = state.cache.get(file);

      compileFile(
        file,
        state.cache,
        {
          sourceMap: state.mergedOptions?.sourceMap ?? !state.isProduction,
          ssr: state.mergedOptions?.ssr ?? false,
        },
        source,
      );

      const newCompiled = state.cache.get(file)!;

      const updateType: HmrUpdateType = detectHmrUpdateType(prevCompiled, newCompiled);

      state.logger.log(`Re-compiled: ${path.relative(state.root, file)} (${updateType})`);

      const virtualId = toVirtualId(file);
      const modules =
        server.moduleGraph.getModulesByFile(virtualId) ?? server.moduleGraph.getModulesByFile(file);

      const hasDelegated = newCompiled.styles?.some(
        (s) =>
          (s.lang !== null && ["scss", "sass", "less", "stylus", "styl"].includes(s.lang)) ||
          s.module !== false,
      );

      if (hasDelegated && updateType === "style-only") {
        const affectedModules: Set<import("vite").ModuleNode> = new Set();
        for (const block of newCompiled.styles ?? []) {
          const params = new URLSearchParams();
          params.set("vue", "");
          params.set("type", "style");
          params.set("index", String(block.index));
          if (block.scoped) params.set("scoped", `data-v-${newCompiled.scopeId}`);
          params.set("lang", block.lang ?? "css");
          if (block.module !== false) {
            params.set("module", typeof block.module === "string" ? block.module : "");
          }
          const styleId = `${file}?${params.toString()}`;
          const styleMods = server.moduleGraph.getModulesByFile(styleId);
          if (styleMods) {
            for (const mod of styleMods) {
              affectedModules.add(mod);
            }
          }
        }
        if (modules) {
          for (const mod of modules) {
            affectedModules.add(mod);
          }
        }
        if (affectedModules.size > 0) {
          return [...affectedModules];
        }
      }

      if (updateType === "style-only" && newCompiled.css && !hasDelegated) {
        server.ws.send({
          type: "custom",
          event: "vize:update",
          data: {
            id: newCompiled.scopeId,
            type: "style-only",
            css: resolveCssImports(
              newCompiled.css,
              file,
              state.cssAliasRules,
              true,
              state.clientViteBase,
            ),
          },
        });
        return [];
      }

      if (modules) {
        return [...modules];
      }
    } catch (e) {
      state.logger.error(`Re-compilation failed for ${file}:`, e);
    }
  }
}

export function handleGenerateBundleHook(
  state: VizePluginState,
  emitFile: (file: { type: "asset"; fileName: string; source: string }) => void,
): void {
  if (!state.extractCss || state.collectedCss.size === 0) {
    return;
  }

  const allCss = Array.from(state.collectedCss.values()).join("\n\n");
  if (allCss.trim()) {
    emitFile({
      type: "asset",
      fileName: "assets/vize-components.css",
      source: allCss,
    });
    state.logger.log(
      `Extracted CSS to assets/vize-components.css (${state.collectedCss.size} components)`,
    );
  }
}
