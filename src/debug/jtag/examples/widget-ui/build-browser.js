#!/usr/bin/env node

/**
 * Browser Build Script with JTAG Path Resolution
 * 
 * Uses the JTAG esbuild configuration to properly resolve @shared, @chatShared, etc.
 */

const esbuild = require('esbuild');
const path = require('path');
const fs = require('fs');

// Plugin to transform JTAG path mappings based on config
function createJTAGPathTransformer() {
  return {
    name: 'jtag-path-transformer',
    setup(build) {
      // Load the JTAG path config
      const jtagPkg = require.resolve('@continuum/jtag/package.json');
      const jtagRoot = path.dirname(jtagPkg);
      
      let pathMappings = {};
      try {
        // Try to load the path config file
        const configPath = path.join(jtagRoot, 'jtag-paths.json');
        if (fs.existsSync(configPath)) {
          const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
          pathMappings = config.pathMappings;
        } else {
          throw new Error('Config file not found');
        }
      } catch (error) {
        console.warn('⚠️ Could not load jtag-paths.json, using fallback mappings');
        // Fallback mappings
        pathMappings = {
          '@shared': 'shared',
          '../../daemons/command-daemon/shared/CommandBase': 'daemons/command-daemon/shared/CommandBase',
          '@chatShared': 'commands/chat/shared',
          '@daemons': 'daemons',
          '@browser': 'browser',
          '@server': 'server',
          '@commandRoot': 'commands'
        };
      }
      
      // Transform imports during load phase
      build.onLoad({ filter: /\.js$/ }, async (args) => {
        if (!args.path.includes('node_modules/@continuum/jtag')) {
          return; // Only transform JTAG package files
        }
        
        console.log(`🔍 ESBuild Plugin: Processing file: ${args.path}`);
        
        let contents = await fs.promises.readFile(args.path, 'utf8');
        let originalContents = contents;
        let transformationCount = 0;
        
        // Transform each path mapping
        for (const [alias, target] of Object.entries(pathMappings)) {
          // Handle both with and without subpaths: @chatShared/ChatTypes and @chatShared
          const aliasRegex = new RegExp(`from ['"]${alias.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?:/([^'"]*?))?['"]`, 'g');
          contents = contents.replace(aliasRegex, (match, subpath) => {
            const relativePath = subpath ? `${target}/${subpath}` : target;
            // Calculate relative path from current file to target  
            const currentFileRelativePath = args.path.replace(jtagRoot + '/dist/', '');
            const currentDir = path.dirname(currentFileRelativePath);
            const targetPath = path.relative(currentDir, relativePath);
            const finalPath = targetPath.startsWith('.') ? targetPath : './' + targetPath;
            
            console.log(`🔄 ESBuild Plugin: Transform ${alias}${subpath ? '/' + subpath : ''} -> ${finalPath}`);
            console.log(`   📁 Current file: ${currentFileRelativePath}`);
            console.log(`   📂 Current dir: ${currentDir}`);
            console.log(`   🎯 Target path: ${relativePath}`);
            console.log(`   ✅ Final import: ${finalPath}`);
            
            transformationCount++;
            return `from "${finalPath}"`;
          });
        }
        
        if (transformationCount > 0) {
          console.log(`✨ ESBuild Plugin: Made ${transformationCount} transformations in ${args.path.split('/').pop()}`);
        } else {
          console.log(`⏭️  ESBuild Plugin: No transformations needed in ${args.path.split('/').pop()}`);
        }
        
        return {
          contents,
          loader: 'js'
        };
      });
    }
  };
}

async function buildBrowser() {
  try {
    await esbuild.build({
      entryPoints: ['src/index.ts'],
      bundle: true,
      outfile: 'dist/index.js',
      format: 'esm',
      target: 'es2020',
      platform: 'browser',
      mainFields: ['browser', 'module', 'main'],
      external: [
        'fs', 'fs/promises', 'path', 'child_process', 'os', 'util', 'crypto',
        'http', 'https', 'net', 'tls', 'url', 'querystring', 'stream', 
        'buffer', 'events', 'assert', 'ws', '../server/JTAGSystemServer'
      ],
      define: {
        'global': 'window'
      },
      plugins: [
        createJTAGPathTransformer()
      ]
    });
    
    console.log('✅ Browser build completed successfully');
  } catch (error) {
    console.error('❌ Browser build failed:', error);
    process.exit(1);
  }
}

buildBrowser();