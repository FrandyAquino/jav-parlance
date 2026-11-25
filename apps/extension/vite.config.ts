import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

const src = (path: string) => fileURLToPath(new URL(`./src/${path}`, import.meta.url));

// The manifest references entries by fixed names, so outputs are not hashed.
export default defineConfig({
  root: fileURLToPath(new URL("./src", import.meta.url)),
  base: "./",
  publicDir: fileURLToPath(new URL("./public", import.meta.url)),
  build: {
    outDir: fileURLToPath(new URL("./dist", import.meta.url)),
    emptyOutDir: true,
    target: "es2022",
    modulePreload: { polyfill: false },
    rolldownOptions: {
      input: {
        popup: src("popup.html"),
        offscreen: src("offscreen.html"),
        background: src("background.ts"),
        "capture-processor": src("capture-processor.ts"),
      },
      output: {
        entryFileNames: "[name].js",
        chunkFileNames: "chunks/[name].js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
  test: {
    root: fileURLToPath(new URL(".", import.meta.url)),
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
