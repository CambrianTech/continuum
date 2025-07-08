#!/usr/bin/env node
/**
 * Custom esbuild script with widget discovery plugin
 */

const esbuild = require('esbuild');
const { widgetDiscoveryPlugin } = require('./esbuild-widget-discovery-plugin.cjs');

async function buildBrowser() {
  try {
    console.log('🏗️ Building browser bundle with widget discovery...');
    
    const result = await esbuild.build({
      entryPoints: ['src/ui/continuum-browser.ts'],
      bundle: true,
      outfile: 'src/ui/continuum-browser.js',
      target: 'es2020',
      format: 'esm',
      sourcemap: true,
      loader: {
        '.css': 'text'
      },
      plugins: [
        widgetDiscoveryPlugin
      ],
      logLevel: 'info'
    });

    console.log('✅ Browser bundle built successfully');
    
    if (result.warnings.length > 0) {
      console.log('⚠️ Build warnings:');
      result.warnings.forEach(warning => {
        console.log(`  ${warning.text}`);
      });
    }
    
  } catch (error) {
    console.error('❌ Build failed:', error);
    process.exit(1);
  }
}

buildBrowser();