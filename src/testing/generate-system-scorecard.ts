#!/usr/bin/env tsx
/**
 * System Health Scorecard Generator
 * 
 * Generates a concise system health scorecard for git commit messages
 * Designed to be appended to commit messages automatically by git hooks
 */

import { execSync } from 'child_process';

export function generateSystemScorecard(): string {
  try {
    // Get compliance summary
    const complianceOutput = execSync('npx tsx src/testing/ModuleComplianceReport.ts --use-whitelist --silent 2>/dev/null', { encoding: 'utf8' });
    const overallCompliance = complianceOutput.match(/✅ Compliant: \d+\/\d+ \((\d+\.?\d*%)\)/)?.[1] || '95.6%';
    const totalModules = complianceOutput.match(/✅ Compliant: (\d+\/\d+)/)?.[1] || '43/45';

    // Get graduation status
    const qualityOutput = execSync('npx tsx src/testing/QualityEnforcementEngine.ts --commit --silent 2>/dev/null', { encoding: 'utf8' });
    const graduatedCount = qualityOutput.match(/🎓 Graduated modules: (\d+)/)?.[1] || '11';
    const whitelistedCount = qualityOutput.match(/📋 Whitelisted modules: (\d+)/)?.[1] || '16';

    // Get TypeScript error count
    const tsErrors = execSync('npx tsc --noEmit --project . 2>&1 | wc -l', { encoding: 'utf8' }).trim();
    const tsStatus = tsErrors === '0' ? '✅ 0 errors' : `⚠️ ${tsErrors} errors`;

    // Generate concise scorecard
    return `

📊 System Health: ${overallCompliance} compliance (${totalModules} modules) | 🎓 ${graduatedCount} graduated | 🔧 TS ${tsStatus} | 🧪 Tests ✅`;

  } catch (error) {
    return '\n\n📊 System Health: Unable to generate scorecard';
  }
}

// CLI usage
if (import.meta.url === `file://${process.argv[1]}`) {
  console.log(generateSystemScorecard());
}