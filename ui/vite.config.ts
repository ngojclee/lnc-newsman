import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { existsSync, readFileSync } from 'node:fs'

// The version shown in the sidebar footer comes from VERSION, so it can never
// disagree with the binary that serves the page. The Docker UI stage copies only
// `ui/`, so it also accepts VERSION beside package.json and finally the package
// version; the value is baked at build time either way.
function buildVersion(): string {
  for (const candidate of [new URL('../VERSION', import.meta.url), new URL('./VERSION', import.meta.url)]) {
    if (existsSync(candidate)) return readFileSync(candidate, 'utf8').trim()
  }
  const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'))
  return String(pkg.version)
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: { __APP_VERSION__: JSON.stringify(buildVersion()) },
  build: { outDir: 'dist', emptyOutDir: true },
  server: { port: 5173, proxy: { '/api': 'http://127.0.0.1:8000', '/health': 'http://127.0.0.1:8000' } },
})
