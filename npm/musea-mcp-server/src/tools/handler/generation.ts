/**
 * MCP tool handlers for code generation.
 *
 * Handles `generate_variants`, `generate_csf`, `generate_docs`,
 * `generate_catalog`, and `get_tokens` tool calls.
 */

import fs from "node:fs";
import path from "node:path";
import { ErrorCode, McpError } from "@modelcontextprotocol/sdk/types.js";
import type { ServerContext, ToolResult } from "./types.js";
import { parseTokensFromPath, generateTokensMarkdown } from "../../tokens.js";

export async function handleGenerateVariants(
  ctx: ServerContext,
  binding: ReturnType<ServerContext["loadNative"]>,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const componentRelPath = args?.componentPath as string;
  if (!componentRelPath) {
    throw new McpError(ErrorCode.InvalidParams, "componentPath is required");
  }
  if (!binding.analyzeSfc) {
    throw new McpError(ErrorCode.InternalError, "analyzeSfc not available in native binding");
  }
  if (!binding.generateVariants) {
    throw new McpError(ErrorCode.InternalError, "generateVariants not available in native binding");
  }

  const absolutePath = path.resolve(ctx.projectRoot, componentRelPath);
  const source = await fs.promises.readFile(absolutePath, "utf-8");

  const analysis = binding.analyzeSfc(source, { filename: absolutePath });
  const props = analysis.props.map((p) => ({
    name: p.name,
    prop_type: p.type,
    required: p.required,
    default_value: p.default_value,
  }));

  const relPath = `./${path.basename(absolutePath)}`;
  const result = binding.generateVariants(relPath, props, {
    max_variants: args?.maxVariants as number | undefined,
    include_default: args?.includeDefault as boolean | undefined,
    include_boolean_toggles: args?.includeBooleanToggles as boolean | undefined,
    include_enum_variants: args?.includeEnumVariants as boolean | undefined,
  });

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          {
            componentName: result.component_name,
            artFileContent: result.art_file_content,
            variants: result.variants.map((v) => ({
              name: v.name,
              isDefault: v.is_default,
              props: v.props,
              description: v.description,
            })),
          },
          null,
          2,
        ),
      },
    ],
  };
}

export async function handleGenerateCsf(
  ctx: ServerContext,
  binding: ReturnType<ServerContext["loadNative"]>,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const artPath = args?.path as string;
  if (!artPath) throw new McpError(ErrorCode.InvalidParams, "path is required");

  const absolutePath = path.resolve(ctx.projectRoot, artPath);
  const source = await fs.promises.readFile(absolutePath, "utf-8");
  const csf = binding.artToCsf(source, { filename: absolutePath });

  return { content: [{ type: "text", text: csf.code }] };
}

export async function handleGenerateDocs(
  ctx: ServerContext,
  binding: ReturnType<ServerContext["loadNative"]>,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const artPath = args?.path as string;
  if (!artPath) throw new McpError(ErrorCode.InvalidParams, "path is required");
  if (!binding.generateArtDoc) {
    throw new McpError(ErrorCode.InternalError, "generateArtDoc not available in native binding");
  }

  const absolutePath = path.resolve(ctx.projectRoot, artPath);
  const source = await fs.promises.readFile(absolutePath, "utf-8");
  const doc = binding.generateArtDoc(
    source,
    { filename: absolutePath },
    {
      include_source: args?.includeSource as boolean | undefined,
      include_templates: args?.includeTemplates as boolean | undefined,
      include_metadata: true,
    },
  );

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          {
            markdown: doc.markdown,
            title: doc.title,
            category: doc.category,
            variantCount: doc.variant_count,
          },
          null,
          2,
        ),
      },
    ],
  };
}

export async function handleGenerateCatalog(
  ctx: ServerContext,
  binding: ReturnType<ServerContext["loadNative"]>,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  if (!binding.generateArtCatalog) {
    throw new McpError(
      ErrorCode.InternalError,
      "generateArtCatalog not available in native binding",
    );
  }

  const arts = await ctx.scanArtFiles();
  const sources: string[] = [];
  for (const [filePath] of arts) {
    const source = await fs.promises.readFile(filePath, "utf-8");
    sources.push(source);
  }

  const catalog = binding.generateArtCatalog(sources, {
    include_source: args?.includeSource as boolean | undefined,
    include_templates: args?.includeTemplates as boolean | undefined,
    include_metadata: true,
  });

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          {
            markdown: catalog.markdown,
            componentCount: catalog.component_count,
            categories: catalog.categories,
            tags: catalog.tags,
          },
          null,
          2,
        ),
      },
    ],
  };
}

export async function handleGetTokens(
  ctx: ServerContext,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const inputPath = args?.tokensPath as string | undefined;
  const format = (args?.format as string) ?? "json";

  let resolvedPath: string | null;
  if (inputPath) {
    resolvedPath = path.resolve(ctx.projectRoot, inputPath);
  } else {
    resolvedPath = await ctx.resolveTokensPath();
  }

  if (!resolvedPath) {
    throw new McpError(
      ErrorCode.InvalidParams,
      "No tokens path provided and none auto-detected. Looked for: tokens/, design-tokens/, style-dictionary/ directories.",
    );
  }

  const categories = await parseTokensFromPath(resolvedPath);

  if (format === "markdown") {
    return { content: [{ type: "text", text: generateTokensMarkdown(categories) }] };
  }

  return {
    content: [{ type: "text", text: JSON.stringify({ categories }, null, 2) }],
  };
}
