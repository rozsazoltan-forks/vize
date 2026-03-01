/**
 * MCP tool handlers for component analysis.
 *
 * Handles `analyze_component` and `get_palette` tool calls.
 */

import fs from "node:fs";
import path from "node:path";
import { ErrorCode, McpError } from "@modelcontextprotocol/sdk/types.js";
import type { ServerContext, ToolResult } from "./types.js";

export async function handleAnalyzeComponent(
  ctx: ServerContext,
  binding: ReturnType<ServerContext["loadNative"]>,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const vuePath = args?.path as string;
  if (!vuePath) throw new McpError(ErrorCode.InvalidParams, "path is required");
  if (!binding.analyzeSfc) {
    throw new McpError(ErrorCode.InternalError, "analyzeSfc not available in native binding");
  }

  const absolutePath = path.resolve(ctx.projectRoot, vuePath);
  const source = await fs.promises.readFile(absolutePath, "utf-8");
  const analysis = binding.analyzeSfc(source, { filename: absolutePath });

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          {
            props: analysis.props.map((p) => ({
              name: p.name,
              type: p.type,
              required: p.required,
              defaultValue: p.default_value,
            })),
            emits: analysis.emits,
          },
          null,
          2,
        ),
      },
    ],
  };
}

export async function handleGetPalette(
  ctx: ServerContext,
  binding: ReturnType<ServerContext["loadNative"]>,
  args: Record<string, unknown> | undefined,
): Promise<ToolResult> {
  const artPath = args?.path as string;
  if (!artPath) throw new McpError(ErrorCode.InvalidParams, "path is required");
  if (!binding.generateArtPalette) {
    throw new McpError(
      ErrorCode.InternalError,
      "generateArtPalette not available in native binding",
    );
  }

  const absolutePath = path.resolve(ctx.projectRoot, artPath);
  const source = await fs.promises.readFile(absolutePath, "utf-8");
  const palette = binding.generateArtPalette(source, { filename: absolutePath });

  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(
          {
            title: palette.title,
            controls: palette.controls.map((c) => ({
              name: c.name,
              control: c.control,
              defaultValue: c.default_value,
              description: c.description,
              required: c.required,
              options: c.options,
              range: c.range,
              group: c.group,
            })),
            groups: palette.groups,
            json: palette.json,
            typescript: palette.typescript,
          },
          null,
          2,
        ),
      },
    ],
  };
}
