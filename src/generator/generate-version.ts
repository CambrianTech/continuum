/**
 * Version Generator
 * Extracts version from package.json and generates shared/version.ts
 */

import { readFileSync } from 'fs';
import { join } from 'path';
import { writeIfChanged } from './core/writeIfChanged';

const packageJsonPath = join(process.cwd(), 'package.json');
const outputPath = join(process.cwd(), 'shared', 'version.ts');

const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf-8'));

const content = `/**
 * Version Constants - Auto-generated from package.json
 * DO NOT EDIT MANUALLY
 */

export const VERSION = '${pkg.version}';
export const PACKAGE_NAME = '${pkg.name}';
`;

const changed = writeIfChanged(outputPath, content);
console.log(changed
  ? `✅ Generated shared/version.ts with version ${pkg.version}`
  : `⏭️  shared/version.ts unchanged (${pkg.version})`);
