/**
 * Quick test script for the audit system
 */

import { ModuleAuditor } from './audit/ModuleAuditor';
import { LintCheck } from './audit/checks/LintCheck';
import { MissingFileCheck } from './audit/checks/MissingFileCheck';
import { OutdatedPatternCheck } from './audit/checks/OutdatedPatternCheck';
import { PackageJsonCheck } from './audit/checks/PackageJsonCheck';
import { ReadmeCheck } from './audit/checks/ReadmeCheck';
import { TestCoverageCheck } from './audit/checks/TestCoverageCheck';

async function main(): Promise<void> {
  console.log('🧪 Testing Audit System\n');

  // Create auditor and register checks
  const auditor = new ModuleAuditor();
  auditor.registerCheck(new LintCheck());
  auditor.registerCheck(new MissingFileCheck());
  auditor.registerCheck(new OutdatedPatternCheck());
  auditor.registerCheck(new PackageJsonCheck());
  auditor.registerCheck(new ReadmeCheck());
  auditor.registerCheck(new TestCoverageCheck());

  // Test with generate/audit command (dogfooding!)
  console.log('Testing with generate/audit command (dogfooding!)...\n');
  const report = await auditor.audit('commands/generate/audit', 'command', {
    fix: true,
  });

  // Display report
  console.log(auditor.formatReport(report));

  // Exit with error if there are remaining errors
  if (report.summary.errors > 0) {
    console.error('\n❌ Audit command still has errors after fix!');
    process.exit(1);
  }

  console.log('\n✅ Audit command passes all checks! 🎉\n');
}

main().catch((error) => {
  console.error('❌ Test failed:', error);
  process.exit(1);
});
