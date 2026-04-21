import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";

const repoRoot = path.resolve(import.meta.dirname, "..");
const cargoToml = readFileSync(path.join(repoRoot, "Cargo.toml"), "utf8");
const currentVersion = cargoToml.match(/^version = "(.+)"$/m)?.[1];

assert.ok(currentVersion, "Failed to read current version from Cargo.toml");

function runBash(script: string) {
  return spawnSync("bash", ["-lc", script], {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["pipe", "pipe", "pipe"],
  });
}

test("release script fails clearly when stdin is not interactive", () => {
  const result = runBash("bash ./scripts/release.sh minor");

  assert.equal(result.status, 1);
  assert.equal(
    result.stderr,
    "Error: Confirmation requires an interactive terminal. Re-run with -y to skip the prompt.\n",
  );
  assert.match(
    result.stdout,
    new RegExp(
      `^Current version: ${currentVersion.replaceAll(".", "\\.")}\\nNew version: .+ \\(tag: v.+\\)\\n\\n$`,
    ),
  );
});

test("confirm_release skips prompting when -y is set", () => {
  const result = runBash(
    "source scripts/release.sh; AUTO_CONFIRM=-y; confirm_release; printf 'confirmed\\n'",
  );

  assert.deepEqual(
    {
      status: result.status,
      stdout: result.stdout,
      stderr: result.stderr,
    },
    {
      status: 0,
      stdout: "confirmed\n",
      stderr: "",
    },
  );
});

test("update_workspace_manifest_versions bumps exact internal crate pins", () => {
  const tempDir = mkdtempSync(path.join(tmpdir(), "release-script-"));
  const manifestPath = path.join(tempDir, "Cargo.toml");

  try {
    writeFileSync(
      manifestPath,
      `[workspace.package]
version = "0.52.0"

[workspace.dependencies]
vize_carton = { path = "crates/vize_carton", version = "=0.52.0" }
vize_relief = { path = "crates/vize_relief", version = "=0.52.0" }
serde = { version = "=1.0.228", features = ["derive"] }
`,
    );

    const result = runBash(
      `source scripts/release.sh; update_workspace_manifest_versions '${manifestPath}' 0.52.0 0.53.0`,
    );

    assert.equal(result.status, 0, result.stderr);

    const updated = readFileSync(manifestPath, "utf8");
    assert.match(updated, /\[workspace\.package\]\nversion = "0\.53\.0"/);
    assert.match(
      updated,
      /vize_carton = \{ path = "crates\/vize_carton", version = "=0\.53\.0" \}/,
    );
    assert.match(
      updated,
      /vize_relief = \{ path = "crates\/vize_relief", version = "=0\.53\.0" \}/,
    );
    assert.match(updated, /serde = \{ version = "=1\.0\.228", features = \["derive"\] \}/);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
});
