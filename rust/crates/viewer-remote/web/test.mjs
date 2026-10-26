// Bundles the TypeScript tests for Node and runs them; nothing is written to the source tree.
import * as esbuild from "esbuild";
import { spawnSync } from "node:child_process";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

const root = path.dirname(new URL(import.meta.url).pathname);
const out = await mkdtemp(path.join(tmpdir(), "nagametv-remote-web-test-"));
try {
  const entries = (await readdir(path.join(root, "test"))).filter((f) => f.endsWith(".test.ts"));
  await esbuild.build({
    absWorkingDir: root,
    entryPoints: entries.map((f) => `test/${f}`),
    bundle: true,
    platform: "node",
    format: "esm",
    outdir: out,
    outExtension: { ".js": ".mjs" },
    logLevel: "warning",
  });
  const files = (await readdir(out)).map((f) => path.join(out, f));
  // Wall-clock formatting is local time; pin it so results do not depend on the host.
  const result = spawnSync(process.execPath, ["--test", ...files], {
    stdio: "inherit",
    env: { ...process.env, TZ: "UTC" },
  });
  process.exitCode = result.status ?? 1;
} finally {
  await rm(out, { recursive: true, force: true });
}
