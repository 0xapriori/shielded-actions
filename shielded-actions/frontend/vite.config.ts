import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  // Base path for GitHub Pages - update this if deploying to a custom domain
  base: process.env.GITHUB_PAGES ? '/shielded-actions/' : '/',
  server: {
    port: 3000
  },
  build: {
    outDir: 'dist',
    sourcemap: true
  }
})
