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

  // Test with hello command
  console.log('Testing with hello command...\n');
  const report = await auditor.audit('commands/hello', 'command');

  // Display report
  console.log(auditor.formatReport(report));

  // Test with --fix
  if (report.summary.fixable > 0) {
    console.log('\n\n🔧 Testing --fix option...\n');
    const fixReport = await auditor.audit('commands/hello', 'command', {
      fix: true,
    });
    console.log(auditor.formatReport(fixReport));
  }
}

main().catch((error) => {
  console.error('❌ Test failed:', error);
  process.exit(1);
});
