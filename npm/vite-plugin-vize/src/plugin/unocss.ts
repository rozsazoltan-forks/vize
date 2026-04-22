import fs from "node:fs";

import {
  isVizeVirtualVueModuleId,
  normalizeVizeVirtualVueModuleId,
} from "../virtual.ts";

type UnoCssLikePlugin = {
  name?: string;
  transform?: (...args: unknown[]) => unknown;
  [bridgePatched]?: boolean;
};

const bridgePatched = Symbol("vize.unocssBridgePatched");

export function patchUnoCssBridge(plugins: UnoCssLikePlugin[]): void {
  for (const plugin of plugins) {
    if (
      !plugin.name?.startsWith("unocss:") ||
      typeof plugin.transform !== "function" ||
      plugin[bridgePatched]
    ) {
      continue;
    }

    const originalTransform = plugin.transform;
    const isExtractionOnly = plugin.name.startsWith("unocss:global");

    plugin.transform = function (
      this: unknown,
      code: string,
      id: string,
      ...args: unknown[]
    ): unknown {
      if (!isVizeVirtualVueModuleId(id)) {
        return originalTransform.call(this, code, id, ...args);
      }

      const normalizedId = normalizeVizeVirtualVueModuleId(id);
      let effectiveCode = code;

      if (isExtractionOnly) {
        try {
          const originalSource = fs.readFileSync(normalizedId.split("?")[0]!, "utf-8");
          effectiveCode = `${code}\n${originalSource}`;
        } catch {
          // Ignore missing virtual sources and keep the compiled code path.
        }
      }

      return originalTransform.call(this, effectiveCode, normalizedId, ...args);
    };

    plugin[bridgePatched] = true;
  }
}
