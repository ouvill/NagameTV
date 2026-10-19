// Bundles the remote page into dist/, which is committed and embedded by Rust.
import * as esbuild from "esbuild";
import { readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";

const root = path.dirname(new URL(import.meta.url).pathname);
const dist = path.join(root, "dist");
await rm(dist, { recursive: true, force: true });

const result = await esbuild.build({
  absWorkingDir: root,
  entryPoints: ["src/main.ts"],
  bundle: true,
  minify: true,
  format: "esm",
  target: ["es2022", "safari16"],
  outdir: "dist/assets",
  entryNames: "app-[hash]",
  legalComments: "none",
  metafile: true,
  write: true,
});

const outputs = Object.keys(result.metafile.outputs).map((file) => path.basename(file));
const script = outputs.find((file) => file.endsWith(".js"));
const style = outputs.find((file) => file.endsWith(".css"));
const html = (await readFile(path.join(root, "index.html"), "utf8"))
  .replace("%SCRIPT%", `assets/${script}`)
  .replace("%STYLE%", `assets/${style}`);
await writeFile(path.join(dist, "index.html"), html);
console.log(`dist/index.html, ${outputs.map((file) => `dist/assets/${file}`).join(", ")}`);
