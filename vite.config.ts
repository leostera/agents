/// <reference types="vitest/config" />
import path from "path";
import { defineConfig } from "vite";
import { frontmanPlugin } from "@frontman-ai/vite";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";
import { storybookTest } from "@storybook/addon-vitest/vitest-plugin";
import { playwright } from "@vitest/browser-playwright";
const dirname =
  typeof __dirname !== "undefined"
    ? __dirname
    : path.dirname(fileURLToPath(import.meta.url));

// More info at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon
const APP_ROOT = "packages/borg-app";
export default defineConfig({
  root: APP_ROOT,
  appType: "spa",
  plugins: [
    frontmanPlugin({
      host: "api.frontman.sh",
    }),
    tailwindcss(),
  ],
  resolve: {
    alias: [
      {
        find: "@",
        replacement: path.resolve(dirname, "packages/borg-ui/src"),
      },
      {
        find: "@/",
        replacement: `${path.resolve(dirname, "packages/borg-ui/src")}/`,
      },
      {
        find: "@borg/ui/index.css",
        replacement: path.resolve(dirname, "packages/borg-ui/src/index.css"),
      },
      {
        find: "@borg/ui/styles.css",
        replacement: path.resolve(dirname, "packages/borg-ui/src/styles.css"),
      },
      {
        find: "@borg/ui",
        replacement: path.resolve(dirname, "packages/borg-ui/src/index.ts"),
      },
      {
        find: "@borg/api",
        replacement: path.resolve(dirname, "packages/borg-api/src/index.ts"),
      },
      {
        find: "@borg/explorer",
        replacement: path.resolve(
          dirname,
          "packages/borg-explorer/src/index.tsx"
        ),
      },
      {
        find: "@borg/i18n",
        replacement: path.resolve(dirname, "packages/borg-i18n/src/index.ts"),
      },
      {
        find: "@borg/devmode",
        replacement: path.resolve(dirname, "packages/borg-devmode/src/index.ts"),
      },
      {
        find: "@borg/onboard",
        replacement: path.resolve(dirname, "packages/borg-onboard/src/index.ts"),
      },
    ],
  },
  build: {
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        entryFileNames: "assets/app.js",
        assetFileNames: (assetInfo) => {
          if (assetInfo.name?.endsWith(".css")) {
            return "assets/app.css";
          }
          return "assets/[name]-[hash][extname]";
        },
      },
    },
  },
  test: {
    projects: [
      {
        extends: true,
        plugins: [
          // The plugin will run tests for the stories defined in your Storybook config
          // See options at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon#storybooktest
          storybookTest({
            configDir: path.join(dirname, "packages/borg-ui/.storybook"),
          }),
        ],
        test: {
          name: "storybook",
          browser: {
            enabled: true,
            headless: true,
            provider: playwright({}),
            instances: [
              {
                browser: "chromium",
              },
            ],
          },
          setupFiles: [".storybook/vitest.setup.js"],
        },
      },
    ],
  },
});
