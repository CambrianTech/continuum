#!/usr/bin/env node

/**
 * Shared Browser Build Script for Examples
 * Usage: node ../../scripts/build-browser-example.js
 */

const esbuild = require('esbuild');
const path = require('path');

async function build() {
  const exampleName = path.basename(process.cwd());
  console.log(`🔧 Building ${exampleName} browser client...`);

  try {
    const result = await esbuild.build({
      entryPoints: ['src/index.ts'],
      bundle: true,
      outdir: 'dist',
      format: 'esm',
      target: 'es2020',
      platform: 'browser',
      sourcemap: true,
      minify: false,
      splitting: false,
      external: [],
      define: {
        'process.env.NODE_ENV': '"production"'
      },
      loader: {
        '.ts': 'ts',
        '.js': 'js'
      },
      resolveExtensions: ['.ts', '.js'],
      alias: {
        '@continuum/jtag': path.resolve(__dirname, '../browser-index.ts')
      },
      write: true,
    });

    if (result.errors.length > 0) {
      console.error('❌ Build errors:', result.errors);
      process.exit(1);
    }

    if (result.warnings.length > 0) {
      console.warn('⚠️ Build warnings:', result.warnings);
    }

    console.log('✅ Browser build completed successfully');

  } catch (error) {
    console.error('❌ Browser build failed:', error);
    process.exit(1);
  }
}

build();