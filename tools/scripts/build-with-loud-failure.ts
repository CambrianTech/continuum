/**
 * Build Script with LOUD Failure Detection
 *
 * STOPS deployment if TypeScript compilation fails
 * NO SILENT FAILURES
 */

import { execSync } from 'child_process';
import { copyFileSync, mkdirSync, existsSync } from 'fs';
import { dirname } from 'path';

console.log('🔨 Building TypeScript with strict error checking...\n');

try {
  // Run TypeScript compilation
  execSync('tsc --project tsconfig.json', {
    stdio: 'inherit',
    encoding: 'utf-8'
  });

  // Copy non-TS runtime assets that ModelRegistry / scripts read by path.
  // tsc doesn't copy JSON — anything that ships next to .ts and is read
  // at runtime via __dirname must be replicated into dist/.
  const assets: Array<[string, string]> = [
    ['shared/models.json', 'dist/src/shared/models.json'],
  ];
  for (const [src, dest] of assets) {
    if (!existsSync(src)) continue;  // Optional asset — skip if absent.
    mkdirSync(dirname(dest), { recursive: true });
    copyFileSync(src, dest);
    console.log(`📦 Copied asset: ${src} → ${dest}`);
  }

  console.log('\n✅ TypeScript compilation succeeded');
  process.exit(0);

} catch (error) {
  console.error('\n❌ ❌ ❌ TYPESCRIPT COMPILATION FAILED ❌ ❌ ❌');
  console.error('🔥 DEPLOYMENT STOPPED - FIX COMPILATION ERRORS FIRST');
  console.error('');
  console.error('This is NOT a silent failure - you MUST fix the errors above');
  console.error('');
  process.exit(1);
}
