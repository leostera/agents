import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react-swc";

const dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  root: dirname,
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(dirname, "src"),
      "@/": `${path.resolve(dirname, "src")}/`,
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: [path.resolve(dirname, "vitest.setup.ts")],
    include: ["src/**/*.vitest.ts", "src/**/*.vitest.tsx"],
    clearMocks: true,
  },
});
