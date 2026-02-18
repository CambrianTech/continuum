/**
 * Build Script with LOUD Failure Detection
 *
 * STOPS deployment if TypeScript compilation fails
 * NO SILENT FAILURES
 */

import { execSync } from 'child_process';

console.log('🔨 Building TypeScript with strict error checking...\n');

try {
  // Run TypeScript compilation
  execSync('tsc --project tsconfig.json', {
    stdio: 'inherit',
    encoding: 'utf-8'
  });

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
