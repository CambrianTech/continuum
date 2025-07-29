#!/usr/bin/env node
/**
 * Converts path alias imports to relative imports in compiled JavaScript files
 * This fixes Node.js runtime resolution issues
 */

const fs = require('fs');
const path = require('path');
const configPath = path.join(__dirname, '../jtag-paths.json');
const distPath = path.join(__dirname, '../dist');

console.log('🔄 Converting path alias imports to relative imports in JavaScript files...');

// Load path mappings config
const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
const { pathMappings } = config;

// Find all JavaScript files in dist recursively
function findJSFiles(dir) {
  const files = [];
  const items = fs.readdirSync(dir);
  
  items.forEach(item => {
    const itemPath = path.join(dir, item);
    const stat = fs.statSync(itemPath);
    
    if (stat.isDirectory()) {
      files.push(...findJSFiles(itemPath));
    } else if (item.endsWith('.js')) {
      files.push(path.relative(distPath, itemPath));
    }
  });
  
  return files;
}

const jsFiles = findJSFiles(distPath);

let totalReplacements = 0;

jsFiles.forEach(file => {
  const filePath = path.join(distPath, file);
  const fileDir = path.dirname(filePath);
  let content = fs.readFileSync(filePath, 'utf8');
  let fileChanged = false;
  let fileReplacements = 0;

  console.log(`\n🔍 Processing: ${file}`);

  // Convert each path alias to relative import
  for (const [alias, relativePath] of Object.entries(pathMappings)) {
    const regex = new RegExp(`from '${alias}/([^']+)'`, 'g');
    const regex2 = new RegExp(`from "${alias}/([^"]+)"`, 'g');
    const regex3 = new RegExp(`from '${alias}'`, 'g');
    const regex4 = new RegExp(`from "${alias}"`, 'g');

    content = content.replace(regex, (match, importPath) => {
      const targetPath = path.join(distPath, relativePath, importPath);
      const relativeImport = path.relative(fileDir, targetPath).replace(/\\/g, '/');
      const finalImport = relativeImport.startsWith('.') ? relativeImport : `./${relativeImport}`;
      fileChanged = true;
      totalReplacements++;
      fileReplacements++;
      console.log(`   🔄 ${alias}/${importPath} -> ${finalImport}`);
      return `from '${finalImport}'`;
    });

    content = content.replace(regex2, (match, importPath) => {
      const targetPath = path.join(distPath, relativePath, importPath);
      const relativeImport = path.relative(fileDir, targetPath).replace(/\\/g, '/');
      const finalImport = relativeImport.startsWith('.') ? relativeImport : `./${relativeImport}`;
      fileChanged = true;
      totalReplacements++;
      fileReplacements++;
      console.log(`   🔄 ${alias}/${importPath} -> ${finalImport}`);
      return `from "${finalImport}"`;
    });

    // Handle imports without subpaths - intelligently detect target type
    const basePath = path.join(distPath, relativePath);
    const indexPath = path.join(basePath, 'index.js');
    const filePath = basePath + '.js';
    
    // Check if this is a direct import (no subpath)
    const hasDirectImport = content.includes(`from '${alias}'`) || content.includes(`from "${alias}"`);
    
    if (hasDirectImport) {
      // Smart detection: prefer index.js if it exists, otherwise use .js file
      const targetPath = fs.existsSync(indexPath) ? indexPath : filePath;
      
      content = content.replace(regex3, () => {
        const relativeImport = path.relative(fileDir, targetPath).replace(/\\/g, '/');
        const finalImport = relativeImport.startsWith('.') ? relativeImport : `./${relativeImport}`;
        fileChanged = true;
        totalReplacements++;
        fileReplacements++;
        console.log(`   🔄 ${alias} -> ${finalImport}`);
        return `from '${finalImport}'`;
      });

      content = content.replace(regex4, () => {
        const relativeImport = path.relative(fileDir, targetPath).replace(/\\/g, '/');
        const finalImport = relativeImport.startsWith('.') ? relativeImport : `./${relativeImport}`;
        fileChanged = true;
        totalReplacements++;
        fileReplacements++;
        console.log(`   🔄 ${alias} -> ${finalImport}`);
        return `from "${finalImport}"`;
      });
    }
  }

  if (fileChanged) {
    fs.writeFileSync(filePath, content);
    console.log(`✅ Updated: ${file} (${fileReplacements} replacements)`);
  } else {
    console.log(`⏭️  No changes: ${file}`);
  }
});

console.log(`🎉 Conversion complete! Made ${totalReplacements} replacements across ${jsFiles.length} files.`);