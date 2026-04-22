import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function readRepoFile(...segments: string[]): string {
  return fs.readFileSync(path.join(root, ...segments), "utf8");
}

test("GitHub workflows opt JavaScript actions into Node 24", () => {
  for (const workflowName of ["check.yml", "deploy-docs.yml", "release.yml"]) {
    const workflow = readRepoFile(".github", "workflows", workflowName);
    assert.match(workflow, /FORCE_JAVASCRIPT_ACTIONS_TO_NODE24:\s*true/);
  }
});

test("deploy-docs deploy job installs MoonBit before running script-mode helpers", () => {
  const workflow = readRepoFile(".github", "workflows", "deploy-docs.yml");
  const deployJob = workflow.slice(workflow.indexOf("\n  deploy:\n"));
  const setupIndex = deployJob.indexOf("- uses: ./.github/actions/setup-moonbit");
  const moonRunIndex = deployJob.indexOf(
    "run: moon run --target native - -- < tools/moon/scripts/github/create_site_structure.mbtx",
  );

  assert.notEqual(setupIndex, -1);
  assert.notEqual(moonRunIndex, -1);
  assert.ok(setupIndex < moonRunIndex);
});

test("setup-moonbit defines explicit Windows and Unix execution paths", () => {
  const action = readRepoFile(".github", "actions", "setup-moonbit", "action.yml");

  assert.match(action, /Setup MSVC toolchain \(Windows\)/);
  assert.match(action, /uses: ilammy\/msvc-dev-cmd@v1/);
  assert.match(action, /Install MoonBit \(Windows\)/);
  assert.match(action, /if: runner\.os == 'Windows'/);
  assert.match(action, /shell: pwsh/);
  assert.match(action, /Install MoonBit \(Unix\)/);
  assert.match(action, /if: runner\.os != 'Windows'/);
  assert.match(action, /shell: bash/);
});

test("setup-moonbit smoke test validates the native async process runtime", () => {
  const installer = readRepoFile(".github", "actions", "setup-moonbit", "install-moonbit.mjs");

  assert.match(installer, /\["run", "-q", "--target", "native", "-", "--"\]/);
  assert.match(installer, /"moonbitlang\/async@0\.16\.8\/process"/);
  assert.match(installer, /@process\.run/);
});

test("setup-moonbit writes both command and shell shims on Windows so bash steps can resolve moon", () => {
  const installer = readRepoFile(".github", "actions", "setup-moonbit", "install-moonbit.mjs");

  assert.match(installer, /const shimMoonCmd = path\.join\(shimDir, "moon\.cmd"\);/);
  assert.match(installer, /const shimMoonShell = path\.join\(shimDir, "moon"\);/);
  assert.match(installer, /fs\.writeFileSync\(\s*shimMoonCmd,/);
  assert.match(installer, /fs\.writeFileSync\(\s*shimMoonShell,/);
});

test("release workflow does not pin a separate hard-coded Node version for VS Code publishing", () => {
  const workflow = readRepoFile(".github", "workflows", "release.yml");

  assert.doesNotMatch(workflow, /node-version:\s*"24\.14\.0"/);
  assert.match(workflow, /node-version-file:\s*"\.node-version"/);
});

test("release workflow overwrites existing GitHub release assets when a tag is re-driven", () => {
  const workflow = readRepoFile(".github", "workflows", "release.yml");

  assert.match(workflow, /uses: softprops\/action-gh-release@v2[\s\S]*overwrite_files:\s*true/);
});

test("cargo config forces the bundled Rust linker for Windows MSVC targets", () => {
  const cargoConfig = readRepoFile(".cargo", "config.toml");

  assert.match(cargoConfig, /\[target\.x86_64-pc-windows-msvc\]\s*linker = "rust-lld"/);
  assert.match(cargoConfig, /\[target\.aarch64-pc-windows-msvc\]\s*linker = "rust-lld"/);
});

test("release workflow runs GitHub helper scripts with the native target on every runner", () => {
  const workflow = readRepoFile(".github", "workflows", "release.yml");

  assert.doesNotMatch(workflow, /MOON_HELPER_TARGET/);
  assert.match(
    workflow,
    /Install cross-compilation tools \(Linux ARM64\)[\s\S]*moon run --target native - -- < tools\/moon\/scripts\/github\/install_cross_compile_tools\.mbtx/,
  );
  assert.match(
    workflow,
    /Create archive \(Windows\)[\s\S]*moon run --target native - -- \$\{\{ matrix\.settings\.target \}\} \$\{\{ matrix\.settings\.archive \}\} vize\.exe < tools\/moon\/scripts\/github\/create_cli_archive\.mbtx/,
  );
  assert.match(workflow, /Build vize-native[\s\S]*moon run --target native - -- npm\/vize-native/);
});
