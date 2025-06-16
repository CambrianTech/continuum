#!/usr/bin/env node
/**
 * Auto-increment version script
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Read package.json
const packagePath = path.join(__dirname, 'package.json');
const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));

// Parse current version
const version = packageData.version.split('.');
const major = parseInt(version[0]);
const minor = parseInt(version[1]);
const patch = parseInt(version[2]);

// Increment patch version
const newPatch = patch + 1;
const newVersion = `${major}.${minor}.${newPatch}`;

// Update package.json
packageData.version = newVersion;
fs.writeFileSync(packagePath, JSON.stringify(packageData, null, 2) + '\n');

console.log(`✅ Version auto-incremented: ${packageData.version} → ${newVersion}`);