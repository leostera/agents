import path from 'path'
import { defineConfig } from 'vite'
import { frontmanPlugin } from '@frontman-ai/vite'

const APP_ROOT = 'packages/borg-app'

export default defineConfig({
  root: APP_ROOT,
  appType: 'spa',
  plugins: [frontmanPlugin({ host: 'api.frontman.sh' })],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'packages/borg-ui/src'),
    },
  },
  build: {
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        entryFileNames: 'assets/app.js',
        assetFileNames: (assetInfo) => {
          if (assetInfo.name?.endsWith('.css')) {
            return 'assets/app.css'
          }
          return 'assets/[name]-[hash][extname]'
        },
      },
    },
  },
})
