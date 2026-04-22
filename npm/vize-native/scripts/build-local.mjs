import { copyFileSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.resolve(scriptDir, "..");
const outputDir = path.join(packageDir, ".artifacts", "native");
const isRelease = process.argv.includes("--release");

rmSync(outputDir, { recursive: true, force: true });
mkdirSync(outputDir, { recursive: true });

const buildArgs = [
  "exec",
  "napi",
  "build",
  "--platform",
  "--manifest-path",
  "../../crates/vize_vitrine/Cargo.toml",
  "-p",
  "vize_vitrine",
  "--features",
  "napi",
  "--output-dir",
  outputDir,
  "--no-js",
];

if (isRelease) {
  buildArgs.splice(4, 0, "--release");
}

const buildResult = spawnSync("pnpm", buildArgs, {
  cwd: packageDir,
  stdio: "inherit",
});

if (buildResult.status !== 0) {
  process.exit(buildResult.status ?? 1);
}

for (const file of readdirSync(packageDir)) {
  if (file.startsWith("vize-vitrine.") && file.endsWith(".node")) {
    rmSync(path.join(packageDir, file), { force: true });
  }
}

for (const file of readdirSync(outputDir)) {
  if (!file.endsWith(".node")) {
    continue;
  }
  copyFileSync(path.join(outputDir, file), path.join(packageDir, file));
}
