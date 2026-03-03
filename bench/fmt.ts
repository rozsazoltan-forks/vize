/**
 * Format Benchmark: Vize (glyph) vs Prettier
 *
 * Usage:
 *   1. Build CLI: mise run build:cli
 *   2. Run benchmark: node --experimental-strip-types bench/fmt.ts
 *
 * Input files are regenerated before each format run to ensure
 * consistent (unformatted) input.
 */

import { existsSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { execSync } from "node:child_process";
import os from "node:os";

const __dirname = dirname(fileURLToPath(import.meta.url));
const INPUT_DIR = join(__dirname, "__in__");
const CPU_COUNT = os.cpus().length;
const VIZE_BIN = join(__dirname, "..", "target", "release", "vize");
const GLOB_PATTERN = join(INPUT_DIR, "*.vue");
const GENERATE_SCRIPT = join(__dirname, "generate.mjs");

// Regenerate input files (ensures fresh, unformatted state)
function regenerateInput(): void {
  execSync(`node ${GENERATE_SCRIPT}`, { stdio: "ignore" });
}

// Initial generation to get file metadata
regenerateInput();

const vueFiles = readdirSync(INPUT_DIR).filter((f) => f.endsWith(".vue"));
if (vueFiles.length === 0) {
  console.error(
    `Error: No .vue files found in ${INPUT_DIR}\ngenerate.mjs failed.`
  );
  process.exit(1);
}

const totalSize = vueFiles.reduce(
  (sum, f) => sum + statSync(join(INPUT_DIR, f)).size,
  0
);

// Format helpers
function formatTime(ms: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)}s`;
  return `${ms.toFixed(0)}ms`;
}

function formatThroughput(fileCount: number, ms: number): string {
  const filesPerSec = (fileCount / ms) * 1000;
  if (filesPerSec >= 1000)
    return `${(filesPerSec / 1000).toFixed(1)}k files/s`;
  return `${filesPerSec.toFixed(0)} files/s`;
}

function formatBytesPerSec(bytes: number, ms: number): string {
  const bps = (bytes / ms) * 1000;
  if (bps >= 1024 * 1024) return `${(bps / 1024 / 1024).toFixed(1)} MB/s`;
  if (bps >= 1024) return `${(bps / 1024).toFixed(1)} KB/s`;
  return `${bps.toFixed(0)} B/s`;
}

// Run shell command, ignoring exit code (formatter may exit non-zero on diffs)
function execIgnoreExit(cmd: string): void {
  try {
    execSync(cmd, { stdio: "ignore" });
  } catch {
    // ignore non-zero exit code
  }
}

// Prettier CLI (inherently single-threaded)
function runPrettier(): number {
  // Warmup
  regenerateInput();
  for (let i = 0; i < 3; i++) {
    execIgnoreExit(`npx prettier --write '${GLOB_PATTERN}'`);
    regenerateInput();
  }

  regenerateInput();
  const start = performance.now();
  execIgnoreExit(`npx prettier --write '${GLOB_PATTERN}'`);
  return performance.now() - start;
}

// Vize (glyph) single-thread
function runVizeFmtSingleThread(): number {
  // Warmup
  regenerateInput();
  for (let i = 0; i < 3; i++) {
    execIgnoreExit(
      `RAYON_NUM_THREADS=1 ${VIZE_BIN} fmt --write '${GLOB_PATTERN}'`
    );
    regenerateInput();
  }

  regenerateInput();
  const start = performance.now();
  execIgnoreExit(
    `RAYON_NUM_THREADS=1 ${VIZE_BIN} fmt --write '${GLOB_PATTERN}'`
  );
  return performance.now() - start;
}

// Vize (glyph) multi-thread
function runVizeFmtMultiThread(): number {
  // Warmup
  regenerateInput();
  for (let i = 0; i < 3; i++) {
    execIgnoreExit(`${VIZE_BIN} fmt --write '${GLOB_PATTERN}'`);
    regenerateInput();
  }

  regenerateInput();
  const start = performance.now();
  execIgnoreExit(`${VIZE_BIN} fmt --write '${GLOB_PATTERN}'`);
  return performance.now() - start;
}

// Main
console.log();
console.log("=".repeat(65));
console.log(" Format Benchmark: glyph vs prettier");
console.log("=".repeat(65));
console.log();
console.log(` Files     : ${vueFiles.length.toLocaleString()} SFC files`);
console.log(` Total Size: ${(totalSize / 1024 / 1024).toFixed(1)} MB`);
console.log(` CPU Cores : ${CPU_COUNT}`);
console.log();
console.log("-".repeat(65));

console.log();

const prettierTime = runPrettier();
console.log(
  `   Prettier (CLI)      : ${formatTime(prettierTime).padStart(8)}  (${formatThroughput(vueFiles.length, prettierTime)}, ${formatBytesPerSec(totalSize, prettierTime)})`
);

let vizeSingle = 0;
let vizeMulti = 0;

if (existsSync(VIZE_BIN)) {
  vizeSingle = runVizeFmtSingleThread();
  const stSpeedup = (prettierTime / vizeSingle).toFixed(1);
  console.log(
    `   Vize glyph (1T)     : ${formatTime(vizeSingle).padStart(8)}  (${formatThroughput(vueFiles.length, vizeSingle)}, ${formatBytesPerSec(totalSize, vizeSingle)})  ${stSpeedup}x faster`
  );

  vizeMulti = runVizeFmtMultiThread();
  const mtSpeedup = (prettierTime / vizeMulti).toFixed(1);
  console.log(
    `   Vize glyph (${CPU_COUNT}T)    : ${formatTime(vizeMulti).padStart(8)}  (${formatThroughput(vueFiles.length, vizeMulti)}, ${formatBytesPerSec(totalSize, vizeMulti)})  ${mtSpeedup}x faster`
  );
} else {
  console.log("   Vize (glyph)  : SKIPPED (vize CLI not found)");
}

// Summary
if (vizeSingle > 0 && vizeMulti > 0) {
  console.log();
  console.log("-".repeat(65));
  console.log();
  console.log(" Summary:");
  console.log();
  const stSpeedup = (prettierTime / vizeSingle).toFixed(1);
  const mtSpeedup = (prettierTime / vizeMulti).toFixed(1);
  console.log(`   Prettier vs Vize ST : ${stSpeedup}x`);
  console.log(`   Prettier vs Vize MT : ${mtSpeedup}x`);
}

console.log();
