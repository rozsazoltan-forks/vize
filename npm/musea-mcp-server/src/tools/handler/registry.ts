/**
 * MCP tool handlers for the component registry.
 *
 * Handles `list_components`, `get_component`, `get_variant`, and
 * `search_components` tool calls.
 */

import fs from "node:fs";
import path from "node:path";
import { ErrorCode, McpError } from "@modelcontextprotocol/sdk/types.js";
import type { ServerContext, ToolResult } from "./types.js";

export async function handleListComponents(
  ctx: ServerContext,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const arts = await ctx.scanArtFiles();
  let results = Array.from(arts.values());

  if (args?.category) {
    results = results.filter(
      (a) => a.category?.toLowerCase() === (args.category as string).toLowerCase(),
    );
  }
  if (args?.tag) {
    results = results.filter((a) =>
      a.tags.some((t) => t.toLowerCase() === (args.tag as string).toLowerCase()),
    );
  }

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          results.map((r) => ({
            path: path.relative(ctx.projectRoot, r.path),
            title: r.title,
            description: r.description,
            component: r.component,
            category: r.category,
            tags: r.tags,
            variantCount: r.variantCount,
          })),
          null,
          2,
        ),
      },
    ],
  };
}

export async function handleGetComponent(
  ctx: ServerContext,
  binding: ReturnType<ServerContext["loadNative"]>,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const artPath = args?.path as string;
  if (!artPath) throw new McpError(ErrorCode.InvalidParams, "path is required");

  const absolutePath = path.resolve(ctx.projectRoot, artPath);
  const source = await fs.promises.readFile(absolutePath, "utf-8");
  const parsed = binding.parseArt(source, { filename: absolutePath });

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          {
            metadata: parsed.metadata,
            variants: parsed.variants.map((v) => ({
              name: v.name,
              template: v.template,
              isDefault: v.is_default,
              skipVrt: v.skip_vrt,
            })),
            hasScriptSetup: parsed.has_script_setup,
            hasScript: parsed.has_script,
            styleCount: parsed.style_count,
          },
          null,
          2,
        ),
      },
    ],
  };
}

export async function handleGetVariant(
  ctx: ServerContext,
  binding: ReturnType<ServerContext["loadNative"]>,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const artPath = args?.path as string;
  const variantName = args?.variant as string;
  if (!artPath || !variantName) {
    throw new McpError(ErrorCode.InvalidParams, "path and variant are required");
  }

  const absolutePath = path.resolve(ctx.projectRoot, artPath);
  const source = await fs.promises.readFile(absolutePath, "utf-8");
  const parsed = binding.parseArt(source, { filename: absolutePath });

  const variant = parsed.variants.find((v) => v.name.toLowerCase() === variantName.toLowerCase());
  if (!variant) {
    throw new McpError(ErrorCode.InvalidParams, `Variant "${variantName}" not found`);
  }

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          {
            name: variant.name,
            template: variant.template,
            isDefault: variant.is_default,
            skipVrt: variant.skip_vrt,
          },
          null,
          2,
        ),
      },
    ],
  };
}

export async function handleSearchComponents(
  ctx: ServerContext,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const query = (args?.query as string)?.toLowerCase();
  if (!query) throw new McpError(ErrorCode.InvalidParams, "query is required");

  const arts = await ctx.scanArtFiles();
  const results = Array.from(arts.values()).filter(
    (a) =>
      a.title.toLowerCase().includes(query) ||
      a.description?.toLowerCase().includes(query) ||
      a.tags.some((t) => t.toLowerCase().includes(query)),
  );

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          results.map((r) => ({
            path: path.relative(ctx.projectRoot, r.path),
            title: r.title,
            description: r.description,
            component: r.component,
            category: r.category,
            tags: r.tags,
          })),
          null,
          2,
        ),
      },
    ],
  };
}
