import { defineConfig } from 'vite';
import { resolve } from 'path';

/**
 * Vite Configuration for Widget UI
 *
 * This runs alongside the existing esbuild setup during migration:
 * - npm run dev:vite - Vite dev server with HMR
 * - npm run build    - Still uses esbuild (production)
 *
 * Incremental migration:
 * 1. Widgets in src/components/ use Vite patterns (signals, reactive)
 * 2. Widgets in widgets/ use legacy patterns
 * 3. Both work together during transition
 */
export default defineConfig({
  root: '.',
  base: '/',

  // Development server configuration
  server: {
    port: 9100,  // Different port from existing 9000
    open: false,
    cors: true,
    // Proxy API requests to the existing JTAG server
    proxy: {
      '/ws': {
        target: 'ws://localhost:9001',
        ws: true
      }
    }
  },

  // Build configuration
  build: {
    outDir: 'dist-vite',
    lib: {
      entry: resolve(__dirname, 'src/vite-entry.ts'),
      name: 'ContinuumWidgets',
      formats: ['es'],
      fileName: 'widgets'
    },
    rollupOptions: {
      // External dependencies that shouldn't be bundled
      external: [],
      output: {
        // Preserve module structure for tree-shaking
        preserveModules: false
      }
    }
  },

  // Resolve configuration
  resolve: {
    alias: {
      '@widgets': resolve(__dirname, '../../widgets'),
      '@system': resolve(__dirname, '../../system'),
      '@shared': resolve(__dirname, '../../shared')
    }
  },

  // Enable TypeScript decorators and other features
  esbuild: {
    target: 'es2020',
    keepNames: true
  }
});
