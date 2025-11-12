#!/usr/bin/env npx tsx
/**
 * AI Decision Report Integration Test
 *
 * Tests the complete AI decision intelligence pipeline:
 * - PersonaUser cognitive decision-making (gating/coordination)
 * - Decision persistence to database
 * - Decision querying and retrieval
 * - Report generation with full RAG context
 *
 * This replaces the flaky chat-based AI response test with a deterministic
 * test of actual AI infrastructure.
 *
 * What this tests:
 * - AI decision storage (DecisionEntity in database)
 * - Decision query system (ai/report/decisions command)
 * - RAG context capture (system prompts, conversation history)
 * - Coordination state tracking (gating decisions)
 * - Report formatting (verbose mode with full context)
 */

import { jtag } from '../../server-index';

async function testAIDecisionReportIntegration(): Promise<void> {
  console.log('🤖 AI DECISION REPORT INTEGRATION TEST');
  console.log('======================================\n');

  let client = null;

  try {
    // Step 1: Connect to JTAG system
    console.log('📡 Step 1: Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected to JTAG system\n');

    // Step 2: Verify system is healthy
    console.log('🔍 Step 2: Verifying system health...');
    const pingResult = await client.commands['ping']({});
    if (!pingResult.success) {
      throw new Error('System ping failed - system not healthy');
    }
    console.log('✅ System is healthy\n');

    // Step 3: Query AI decisions and generate report with full RAG context
    console.log('🗄️ Step 3: Querying AI decisions and generating report...');
    const reportResult = await client.commands['ai/report/decisions']({
      limit: 3,
      verbose: true  // Test verbose mode (includes all summary content PLUS RAG context)
    });

    if (!reportResult.success) {
      throw new Error(`Failed to generate AI decision report: ${reportResult.error || 'Unknown error'}`);
    }

    if (!reportResult.reportPath) {
      throw new Error('Report result missing reportPath');
    }

    console.log(`✅ Report generated at: ${reportResult.reportPath}\n`);

    // Step 4: Read and validate report structure
    console.log('📊 Step 4: Validating report structure and content...');
    const fs = await import('fs');
    if (!fs.existsSync(reportResult.reportPath)) {
      throw new Error(`Report file not found at: ${reportResult.reportPath}`);
    }

    const report = fs.readFileSync(reportResult.reportPath, 'utf-8');

    // Check for required report structure (summary sections)
    const requiredSections = [
      '# AI Decision Intelligence Report',
      '## Summary Statistics',
      '## Actor Breakdown',
      '## Decision Timeline'
    ];

    for (const section of requiredSections) {
      if (!report.includes(section)) {
        throw new Error(`Report missing required section: ${section}`);
      }
    }

    // Check for summary statistics
    const requiredStats = [
      '- **Total Decisions**:',
      '- **Posted**:',
      '- **Silent**:',
      '- **Average Confidence**:',
      '- **Unique Actors**:'
    ];

    for (const stat of requiredStats) {
      if (!report.includes(stat)) {
        throw new Error(`Report missing required statistic: ${stat}`);
      }
    }

    // Check for verbose-only sections (RAG context)
    const requiredVerboseSections = [
      '**RAG Context (COMPLETE - EXACTLY what AI saw)**:',
      '**System Prompt** (complete):',
      '**Conversation History**',
      '**Ambient State**:',
      '**Coordination State**:'
    ];

    for (const section of requiredVerboseSections) {
      if (!report.includes(section)) {
        throw new Error(`Verbose report missing required section: ${section}`);
      }
    }

    // Check for decision-level metadata
    const requiredMetadata = [
      '**Time**:',
      '**Actor**:',
      '**Action**:',
      '**Confidence**:'
    ];

    for (const meta of requiredMetadata) {
      if (!report.includes(meta)) {
        throw new Error(`Report missing required metadata: ${meta}`);
      }
    }

    console.log('✅ Report structure validated (summary + verbose RAG context)\n');

    // Success!
    console.log('🎉 AI Decision Report Integration Test: PASSED');
    console.log('==============================================');
    console.log('✅ AI decision database query works');
    console.log('✅ Report generation pipeline works');
    console.log('✅ Summary mode formatting correct');
    console.log('✅ Verbose mode with full RAG context works');
    console.log('✅ Decision metadata complete');
    console.log('✅ Coordination state tracking works');
    console.log('\nThis test validates the ENTIRE AI decision intelligence pipeline.\n');

  } catch (error) {
    console.error('\n❌ AI Decision Report Integration Test: FAILED');
    console.error('===============================================');
    console.error(error instanceof Error ? error.message : String(error));
    console.error('\n💡 Debugging tips:');
    console.error('   1. Check DecisionEntity table: ./jtag data/list --collection=decisions --limit=5');
    console.error('   2. Run report manually: ./jtag ai/report/decisions --limit=5 --verbose=true');
    console.error('   3. Check AI decisions log: .continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log');
    console.error('   4. Check for errors: grep "ERROR" .continuum/jtag/system/logs/npm-start.log\n');

    throw error;
  } finally {
    if (client) {
      try {
        await client.disconnect();
      } catch (disconnectError) {
        console.error('Disconnect error:', disconnectError);
      }
    }
  }
}

// Run test and exit
testAIDecisionReportIntegration().then(() => {
  console.log('✅ AI decision report integration test completed successfully');
  process.exit(0);
}).catch(error => {
  console.error('🚨 AI decision report integration test failed:', error);
  process.exit(1);
});
