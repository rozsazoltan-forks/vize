import type { VizeOptions } from "@vizejs/vite-plugin";

function normalizeUrlPrefix(value: string): string {
  const withLeadingSlash = value.startsWith("/") ? value : `/${value}`;
  return withLeadingSlash.endsWith("/") ? withLeadingSlash : `${withLeadingSlash}/`;
}

export function buildNuxtDevAssetBase(baseURL = "/", buildAssetsDir = "/_nuxt/"): string {
  const normalizedBase = normalizeUrlPrefix(baseURL);
  const normalizedAssetsDir = normalizeUrlPrefix(buildAssetsDir);
  return normalizedBase === "/"
    ? normalizedAssetsDir
    : normalizeUrlPrefix(`${normalizedBase}${normalizedAssetsDir.replace(/^\//, "")}`);
}

export function buildNuxtCompilerOptions(
  rootDir: string,
  baseURL = "/",
  buildAssetsDir = "/_nuxt/",
): Pick<VizeOptions, "devUrlBase" | "root"> {
  return {
    devUrlBase: buildNuxtDevAssetBase(baseURL, buildAssetsDir),
    root: rootDir,
  };
}

export function isVizeVirtualVueModuleId(id: string): boolean {
  return id.startsWith("\0") && /\.vue\.ts(?:\?|$)/.test(id);
}

export function normalizeVizeVirtualVueModuleId(id: string): string {
  const withoutPrefix = id.startsWith("\0vize-ssr:") ? id.slice("\0vize-ssr:".length) : id.slice(1);
  return withoutPrefix.replace(/\.ts(?=\?|$)/, "");
}
