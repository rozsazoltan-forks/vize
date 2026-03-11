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
