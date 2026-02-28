import type { StorybookConfig } from '@storybook/react-vite'
import path, { dirname } from 'path'
import { fileURLToPath } from 'url'
import tailwindcss from '@tailwindcss/vite'

const storybookDir = path.dirname(fileURLToPath(import.meta.url))
const uiSrcDir = path.resolve(storybookDir, '../src')

const config: StorybookConfig = {
  framework: getAbsolutePath("@storybook/react-vite"),
  stories: ['../src/**/*.stories.@(ts|tsx)'],
  addons: [
    getAbsolutePath("@storybook/addon-docs"),
    getAbsolutePath("@storybook/addon-a11y"),
    getAbsolutePath("@storybook/addon-vitest")
  ],
  viteFinal: async (config) => {
    config.plugins = [...(config.plugins ?? []), tailwindcss()]
    config.esbuild = {
      ...(config.esbuild ?? {}),
      jsx: 'automatic',
    }
    config.resolve = config.resolve ?? {}
    const existingAliases = Array.isArray(config.resolve.alias)
      ? config.resolve.alias
      : Object.entries(config.resolve.alias ?? {}).map(([find, replacement]) => ({
          find,
          replacement,
        }))

    config.resolve.alias = [
      ...existingAliases,
      { find: '@', replacement: uiSrcDir },
      { find: '@/', replacement: `${uiSrcDir}/` },
    ]
    return config
  },
}

export default config

function getAbsolutePath(value: string): any {
  return dirname(fileURLToPath(import.meta.resolve(`${value}/package.json`)))
}
