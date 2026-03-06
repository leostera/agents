import path from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react-swc";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

const dirname =
  typeof __dirname !== "undefined"
    ? __dirname
    : path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [tailwindcss(), react()],
  envPrefix: ["VITE_", "BORG_"],
  resolve: {
    alias: [
      {
        find: "@",
        replacement: path.resolve(dirname, "../../packages/borg-ui/src"),
      },
      {
        find: "@/",
        replacement: `${path.resolve(dirname, "../../packages/borg-ui/src")}/`,
      },
      {
        find: "@borg/ui/index.css",
        replacement: path.resolve(dirname, "../../packages/borg-ui/src/index.css"),
      },
      {
        find: "@borg/ui/styles.css",
        replacement: path.resolve(dirname, "../../packages/borg-ui/src/styles.css"),
      },
      {
        find: "@borg/ui",
        replacement: path.resolve(dirname, "../../packages/borg-ui/src/index.ts"),
      },
      {
        find: "@borg/graphql-client",
        replacement: path.resolve(
          dirname,
          "../../packages/borg-graphql-client/src/index.ts"
        ),
      },
    ],
  },
});
