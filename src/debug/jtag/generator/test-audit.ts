/**
 * Quick test script for the audit system
 */

import { ModuleAuditor } from './audit/ModuleAuditor';
import { LintCheck } from './audit/checks/LintCheck';

async function main(): Promise<void> {
  console.log('🧪 Testing Audit System\n');

  // Create auditor and register checks
  const auditor = new ModuleAuditor();
  auditor.registerCheck(new LintCheck());

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
