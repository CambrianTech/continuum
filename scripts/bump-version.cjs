#!/usr/bin/env node
/**
 * Auto-increment build version
 * 
 * Usage:
 *   node scripts/bump-version.js
 *   npm run version:bump
 */

const fs = require('fs');
const path = require('path');

function bumpVersion() {
  const packagePath = path.join(__dirname, '..', 'package.json');
  const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
  
  // Parse current version: "0.2.1847" -> [0, 2, 1847]
  const versionParts = packageJson.version.split('.').map(Number);
  const [major, minor, build] = versionParts;
  
  // Increment build number
  const newBuild = build + 1;
  const newVersion = `${major}.${minor}.${newBuild}`;
  
  // Update package.json
  packageJson.version = newVersion;
  fs.writeFileSync(packagePath, JSON.stringify(packageJson, null, 2) + '\n');
  
  console.log(`🚀 Version bumped: ${packageJson.version.split('.').slice(0, -1).join('.')}.${build} → ${newVersion}`);
  console.log(`📦 Build: ${newBuild}`);
  
  return newVersion;
}

// Run if called directly
if (require.main === module) {
  bumpVersion();
}

module.exports = { bumpVersion };