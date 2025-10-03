/**
 * Version Generator
 * Extracts version from package.json and generates shared/version.ts
 */

import { readFileSync, writeFileSync } from 'fs';
import { join } from 'path';

const packageJsonPath = join(process.cwd(), 'package.json');
const outputPath = join(process.cwd(), 'shared', 'version.ts');

const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf-8'));

const timestamp = new Date().toISOString();

const content = `/**
 * Version Constants - Auto-generated from package.json
 * Generated on ${timestamp}
 * DO NOT EDIT MANUALLY
 */

export const VERSION = '${pkg.version}';
export const PACKAGE_NAME = '${pkg.name}';
`;

writeFileSync(outputPath, content, 'utf-8');
console.log(`✅ Generated shared/version.ts with version ${pkg.version}`);
