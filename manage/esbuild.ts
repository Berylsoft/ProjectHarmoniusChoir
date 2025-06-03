/// <reference lib="deno.ns" />

import * as esbuild from "npm:esbuild";
import { denoPlugins } from "jsr:@luca/esbuild-deno-loader";
import * as path from "jsr:@std/path";

const srcRoot = "./src";
const staticRoot = "./static";
const distRoot = "./dist";

function src(target: string): string {
  return path.join(srcRoot, target);
}

function dist(target: string): string {
  return path.join(distRoot, target);
}

console.log("bundling");
await esbuild.build({
  plugins: [...denoPlugins()],

  jsx: "transform",
  jsxFactory: "buildJsx",
  jsxFragment: "jsxFragment",
  inject: [src("libs/buildJsx.ts")],

  entryPoints: [src("main.tsx")],
  outfile: dist("index.js"),

  target: "esnext",

  bundle: true,
  format: "iife",

  minify: true,
  sourcemap: true,
});

async function buildCss(source: string, target: string) {
  await esbuild.build({
    entryPoints: [src(source)],
    outfile: dist(target),

    bundle: true,
    minify: true,

    sourcemap: true,
  });
}
await buildCss("main.css", "index.css");
await buildCss("libs/normalize.css", "libs/normalize.css");

esbuild.stop();

function lsRec(root: string): string[] {
  const entries = [];
  for (const { name, isDirectory } of Deno.readDirSync(root)) {
    if (isDirectory) {
      for (const e of lsRec(path.join(root, name))) {
        entries.push(path.join(name, e));
      }
    } else {
      entries.push(name);
    }
  }
  return entries;
}

for (const entry of lsRec(staticRoot)) {
  const src = path.join(staticRoot, entry);
  const dst = path.join(distRoot, entry);

  console.log(`Copying ${src} to ${dst}`);
  try {
    Deno.mkdirSync(path.parse(dst).dir, { recursive: true });
  } catch (e) {
    if (!(e instanceof Deno.errors.AlreadyExists)) throw e;
  }
  Deno.copyFileSync(src, dst);
}
