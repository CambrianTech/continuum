import { defineConfig } from 'vite';
import { resolve } from 'path';

/**
 * Vite Configuration for Widget UI - REPLACES esbuild
 *
 * This is the main browser bundler for the Positron widget system.
 * It handles:
 * - TypeScript compilation
 * - SCSS → CSS compilation (native, no separate compile-sass.ts needed)
 * - Bundle optimization
 * - Source maps
 *
 * Usage:
 *   npm run build:browser   → Production build (dist/index.js)
 *   npm run dev:vite        → Dev server with HMR (port 9100)
 */
export default defineConfig({
  root: '.',
  base: '/',

  // Development server (optional - main app still uses port 9000)
  server: {
    port: 9100,
    open: false,
    cors: true,
    // Proxy WebSocket to main JTAG server
    proxy: {
      '/ws': {
        target: 'ws://localhost:9001',
        ws: true
      }
    }
  },

  // Build configuration - matches esbuild output
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: true,
    minify: false, // Match esbuild settings
    target: 'es2020',

    // Library mode for browser bundle
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
      formats: ['es'],
      fileName: () => 'index.js' // Match esbuild output name
    },

    rollupOptions: {
      output: {
        // Single bundle (no code splitting)
        inlineDynamicImports: true,
        // Ensure CSS is extracted
        assetFileNames: '[name][extname]'
      }
    }
  },

  // CSS/SCSS handling - replaces compile-sass.ts for widgets
  css: {
    preprocessorOptions: {
      scss: {
        // Add shared SCSS variables/mixins path
        includePaths: [
          resolve(__dirname, '../../widgets/shared/styles')
        ]
      }
    }
  },

  // Path resolution - matches existing esbuild aliases
  resolve: {
    alias: {
      // Main JTAG package alias (browser build)
      '@continuum/jtag': resolve(__dirname, '../../browser-index.ts'),

      // Widget system aliases
      '@widgets': resolve(__dirname, '../../widgets'),
      '@system': resolve(__dirname, '../../system'),
      '@shared': resolve(__dirname, '../../shared'),
      '@commands': resolve(__dirname, '../../commands'),
      '@daemons': resolve(__dirname, '../../daemons')
    }
  },

  // TypeScript handling
  esbuild: {
    target: 'es2020',
    keepNames: true
  },

  // Define process.env for browser compatibility
  define: {
    'process.env.NODE_ENV': JSON.stringify('production')
  }
});
