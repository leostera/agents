import { defineConfig } from "vite";

export default defineConfig({
  build: {
    lib: {
      entry: "./src/index.ts",
      formats: ["iife"],
      name: "BorgAgentSdk",
      fileName: () => "borg-agent-sdk.min.js",
    },
    minify: "esbuild",
    outDir: "dist",
    emptyOutDir: true,
  },
});
