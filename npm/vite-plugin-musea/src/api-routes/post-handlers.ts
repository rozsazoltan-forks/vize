/**
 * POST route handlers for the Musea gallery API.
 *
 * Handles preview-with-props, generate, and run-vrt endpoints.
 */

import type { ServerResponse } from "node:http";
import path from "node:path";

import type { ApiRoutesContext, SendJson, SendError } from "./index.js";
import { generatePreviewModuleWithProps } from "../preview/index.js";
import { toPascalCase } from "../utils.js";

/** POST /api/preview-with-props */
export function handlePreviewWithProps(
  ctx: ApiRoutesContext,
  body: string,
  res: ServerResponse,
  sendJson: SendJson,
  sendError: SendError,
): void {
  try {
    const { artPath: reqArtPath, variantName, props: propsOverride } = JSON.parse(body);
    const art = ctx.artFiles.get(reqArtPath);
    if (!art) {
      sendError("Art not found", 404);
      return;
    }

    const variant = art.variants.find((v) => v.name === variantName);
    if (!variant) {
      sendError("Variant not found", 404);
      return;
    }

    const variantComponentName = toPascalCase(variant.name);
    const moduleCode = generatePreviewModuleWithProps(
      art,
      variantComponentName,
      variant.name,
      propsOverride,
      ctx.resolvedPreviewCss,
      ctx.resolvedPreviewSetup,
    );
    res.setHeader("Content-Type", "application/javascript");
    res.end(moduleCode);
  } catch (e) {
    sendError(e instanceof Error ? e.message : String(e));
  }
}

/** POST /api/generate */
export async function handleGenerate(
  body: string,
  sendJson: SendJson,
  sendError: SendError,
): Promise<void> {
  try {
    const { componentPath: reqComponentPath, options: autogenOptions } = JSON.parse(body);
    const { generateArtFile: genArt } = await import("../autogen/index.js");
    const result = await genArt(reqComponentPath, autogenOptions);
    sendJson({
      generated: true,
      componentName: result.componentName,
      variants: result.variants,
      artFileContent: result.artFileContent,
    });
  } catch (e) {
    sendError(e instanceof Error ? e.message : String(e));
  }
}

/** POST /api/run-vrt */
export async function handleRunVrt(
  ctx: ApiRoutesContext,
  body: string,
  sendJson: SendJson,
  sendError: SendError,
): Promise<void> {
  try {
    const { artPath, updateSnapshots } = JSON.parse(body);
    const { MuseaVrtRunner } = await import("../vrt.js");

    const runner = new MuseaVrtRunner({
      snapshotDir: path.resolve(ctx.config.root, ".vize/snapshots"),
    });

    const port = ctx.getDevServerPort();
    const baseUrl = `http://localhost:${port}`;

    let artsToTest = Array.from(ctx.artFiles.values());
    if (artPath) {
      artsToTest = artsToTest.filter((a) => a.path === artPath);
    }

    await runner.start();
    const results = await runner.runTests(artsToTest, baseUrl, {
      updateSnapshots,
    });
    const summary = runner.getSummary(results);
    await runner.stop();

    sendJson({
      success: true,
      summary,
      results: results.map((r) => ({
        artPath: r.artPath,
        variantName: r.variantName,
        viewport: r.viewport.name,
        passed: r.passed,
        isNew: r.isNew,
        diffPercentage: r.diffPercentage,
        error: r.error,
      })),
    });
  } catch (e) {
    sendError(e instanceof Error ? e.message : String(e));
  }
}
