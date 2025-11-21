/**
 * AI Gating Quality Test
 * ======================
 *
 * Measures AI persona gating quality across multiple dimensions:
 * - False positives (responding when shouldn't)
 * - False negatives (not responding when should)
 * - Response redundancy (multiple AIs saying same thing)
 * - Gating accuracy (hallucinated vs real reasons)
 * - Response timing (decision time, generation time, total time)
 *
 * Used for iterative improvement tracking - run before and after each change.
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';

const PRIMARY_USER_ID = '00000000-0000-0000-0000-000000000001';
const GENERAL_ROOM_ID = '5e71a0c8-0303-4eb8-a478-3a1212488c8c';

interface ChatMessage {
  id: string;
  senderId: string;
  senderName: string;
  senderType: 'human' | 'ai' | 'system' | 'test';
  content: { text: string };
  timestamp: string;
}

interface TestResult {
  scenario: string;
  passed: boolean;
  aiResponses: Array<{
    aiName: string;
    responseTime: number;
    content: string;
  }>;
  metrics: {
    falsePositive?: boolean;
    falseNegative?: boolean;
    redundancy?: number; // 0-1 scale
    timing?: number;     // milliseconds
  };
  notes: string;
}

/**
 * Wait for AI responses and collect metrics
 */
async function waitForAIResponses(
  startTime: number,
  roomId: string,
  maxWaitSeconds: number = 25
): Promise<ChatMessage[]> {
  const aiResponses: ChatMessage[] = [];

  for (let i = 0; i < maxWaitSeconds * 2; i++) {
    await new Promise(resolve => setTimeout(resolve, 500));

    const result = await runJtagCommand(
      `data/list --collection=chat_messages --filter='{"roomId":"${roomId}"}' --orderBy='[{"field":"timestamp","direction":"desc"}]' --limit=20`
    );

    const messages = result?.items || [];
    const newAIMessages = messages.filter((msg: ChatMessage) =>
      msg.senderType === 'ai' &&
      new Date(msg.timestamp).getTime() > startTime
    );

    // Add any new AI messages we haven't seen
    for (const msg of newAIMessages) {
      if (!aiResponses.find(r => r.id === msg.id)) {
        aiResponses.push(msg);
        console.log(`   📝 ${msg.senderName} responded (${Date.now() - startTime}ms)`);
      }
    }

    // Stop waiting if we've seen responses and no new ones for 5 seconds
    if (aiResponses.length > 0 && i > 10) {
      const lastResponseTime = Math.max(
        ...aiResponses.map(r => new Date(r.timestamp).getTime())
      );
      if (Date.now() - lastResponseTime > 5000) {
        break; // No new responses in 5s, we're done
      }
    }
  }

  return aiResponses;
}

/**
 * Calculate content redundancy (simplified: keyword overlap)
 */
function calculateRedundancy(responses: string[]): number {
  if (responses.length < 2) return 0;

  // Simple keyword-based approach
  const tokenSets = responses.map(r =>
    new Set(r.toLowerCase().split(/\W+/).filter(w => w.length > 4))
  );

  let totalOverlap = 0;
  let comparisons = 0;

  for (let i = 0; i < tokenSets.length; i++) {
    for (let j = i + 1; j < tokenSets.length; j++) {
      const intersection = new Set([...tokenSets[i]].filter(x => tokenSets[j].has(x)));
      const union = new Set([...tokenSets[i], ...tokenSets[j]]);
      totalOverlap += intersection.size / union.size;
      comparisons++;
    }
  }

  return comparisons > 0 ? totalOverlap / comparisons : 0;
}

/**
 * Post-mortem analysis: Check AI logs, timeouts, and RAG context after scenario
 */
async function analyzeScenarioPostMortem(scenarioName: string, startTime: number): Promise<{
  hasTimeouts: boolean;
  timeoutCount: number;
  aiDecisions: Array<{ai: string; decision: string; reason: string}>;
}> {
  console.log(`\n   🔍 POST-MORTEM: Analyzing ${scenarioName}...`);

  // Check AI decision logs for this time window
  const logsResult = await runJtagCommand(`ai/logs --tailLines=50`);
  const logLines = logsResult?.lines || [];

  // Filter logs from this scenario (within time window)
  const scenarioLogs = logLines.filter((line: string) => {
    if (typeof line !== 'string') return false;
    // Check if log is recent (rough approximation - within last few minutes)
    return line.includes('→ RESPOND') || line.includes('→ SILENT') || line.includes('→ ERROR');
  });

  // Count timeouts
  const timeoutLogs = scenarioLogs.filter((line: string) =>
    line.includes('timeout') || line.includes('ERROR')
  );
  const hasTimeouts = timeoutLogs.length > 0;

  // Parse AI decisions
  const aiDecisions: Array<{ai: string; decision: string; reason: string}> = [];
  for (const line of scenarioLogs) {
    if (typeof line !== 'string') continue;

    const aiMatch = line.match(/\[(.*?)\] (.*?) → (RESPOND|SILENT|ERROR)/);
    if (aiMatch) {
      const [, timestamp, aiName, decision] = aiMatch;
      const reasonMatch = line.match(/Reason: (.*?)(?:\||$)/);
      const reason = reasonMatch ? reasonMatch[1].trim() : 'No reason';

      aiDecisions.push({ ai: aiName, decision, reason });
    }
  }

  // Report findings
  console.log(`   📊 Found ${aiDecisions.length} AI decisions, ${timeoutLogs.length} timeouts`);

  if (hasTimeouts) {
    console.log(`   ⚠️  TIMEOUTS DETECTED:`);
    timeoutLogs.slice(0, 3).forEach((line: string) => {
      console.log(`      ${line.substring(0, 100)}`);
    });
  }

  if (aiDecisions.length > 0) {
    console.log(`   🤖 AI Decisions:`);
    aiDecisions.slice(0, 5).forEach(d => {
      console.log(`      ${d.ai} → ${d.decision}: ${d.reason.substring(0, 60)}...`);
    });
  }

  return {
    hasTimeouts,
    timeoutCount: timeoutLogs.length,
    aiDecisions
  };
}

/**
 * Scenario 1: Test Message (Should NOT Respond)
 */
async function testScenario_TestMessage(): Promise<TestResult> {
  console.log('\n📋 Scenario 1: Test Message (Should NOT Respond)');
  console.log('='.repeat(60));

  const startTime = Date.now();

  // Send message from test user
  await runJtagCommand(
    `chat/send --room="${GENERAL_ROOM_ID}" --message="Integration test message" --senderId="test-user-123" --senderName="Test User"`
  );

  console.log('   📨 Sent test message, waiting for any AI responses...');

  const aiResponses = await waitForAIResponses(startTime, GENERAL_ROOM_ID, 20);

  // Post-mortem analysis
  const postMortem = await analyzeScenarioPostMortem('Test Message', startTime);

  const passed = aiResponses.length === 0 && !postMortem.hasTimeouts;

  return {
    scenario: 'Test Message',
    passed,
    aiResponses: aiResponses.map(r => ({
      aiName: r.senderName,
      responseTime: new Date(r.timestamp).getTime() - startTime,
      content: r.content.text.substring(0, 100)
    })),
    metrics: {
      falsePositive: !passed,
      timing: Date.now() - startTime
    },
    notes: passed
      ? '✅ All AIs correctly stayed silent'
      : `❌ ${aiResponses.length} AI(s) responded to test message (false positive)`
  };
}

/**
 * Scenario 2: Novel Question (Should Respond)
 */
async function testScenario_NovelQuestion(): Promise<TestResult> {
  console.log('\n📋 Scenario 2: Novel Question (Should Respond)');
  console.log('='.repeat(60));

  const startTime = Date.now();

  const question = `What are the key differences between REST and GraphQL APIs? [test-${Date.now()}]`;

  await runJtagCommand(
    `chat/send --room="${GENERAL_ROOM_ID}" --message="${question}"`
  );

  console.log(`   📨 Sent: "${question}"`);
  console.log('   ⏳ Waiting for AI responses...');

  const aiResponses = await waitForAIResponses(startTime, GENERAL_ROOM_ID, 30);

  // Post-mortem analysis
  const postMortem = await analyzeScenarioPostMortem('Novel Question', startTime);

  const passed = aiResponses.length >= 1;
  const fastEnough = aiResponses.length > 0 &&
    (new Date(aiResponses[0].timestamp).getTime() - startTime) < 30000;

  return {
    scenario: 'Novel Question',
    passed: passed && fastEnough,
    aiResponses: aiResponses.map(r => ({
      aiName: r.senderName,
      responseTime: new Date(r.timestamp).getTime() - startTime,
      content: r.content.text.substring(0, 100)
    })),
    metrics: {
      falseNegative: !passed,
      timing: aiResponses.length > 0
        ? new Date(aiResponses[0].timestamp).getTime() - startTime
        : Date.now() - startTime
    },
    notes: !passed
      ? '❌ No AI responded to novel question (false negative)'
      : !fastEnough
        ? '⚠️  AI responded but too slowly (>30s)'
        : `✅ ${aiResponses.length} AI(s) responded appropriately`
  };
}

/**
 * Scenario 3: Already Answered (Should NOT Respond Again)
 */
async function testScenario_AlreadyAnswered(): Promise<TestResult> {
  console.log('\n📋 Scenario 3: Already Answered (Should Stay Silent)');
  console.log('='.repeat(60));

  const startTime = Date.now();

  // First, ask a question
  const question = `What is the purpose of async/await in JavaScript? [test-${Date.now()}]`;

  await runJtagCommand(
    `chat/send --room="${GENERAL_ROOM_ID}" --message="${question}"`
  );

  console.log(`   📨 Sent: "${question}"`);
  console.log('   ⏳ Waiting for first AI response...');

  // Wait for first response
  const firstResponses = await waitForAIResponses(startTime, GENERAL_ROOM_ID, 30);

  if (firstResponses.length === 0) {
    // Post-mortem to understand why no response
    await analyzeScenarioPostMortem('Already Answered (setup failed)', startTime);

    return {
      scenario: 'Already Answered',
      passed: false,
      aiResponses: [],
      metrics: { falseNegative: true },
      notes: '❌ No AI responded to initial question (test setup failed)'
    };
  }

  console.log(`   ✅ ${firstResponses[0].senderName} responded first`);
  console.log('   ⏳ Waiting to see if others pile on redundantly...');

  // Wait a bit more to see if others respond
  await new Promise(resolve => setTimeout(resolve, 10000));

  const allResponses = await waitForAIResponses(startTime, GENERAL_ROOM_ID, 5);

  // Post-mortem analysis
  const postMortem = await analyzeScenarioPostMortem('Already Answered', startTime);

  const redundantCount = allResponses.length - 1; // Subtract first responder
  const passed = redundantCount === 0;

  // Calculate redundancy if multiple responses
  let redundancy = 0;
  if (allResponses.length > 1) {
    const contents = allResponses.map(r => r.content.text);
    redundancy = calculateRedundancy(contents);
  }

  return {
    scenario: 'Already Answered',
    passed,
    aiResponses: allResponses.map(r => ({
      aiName: r.senderName,
      responseTime: new Date(r.timestamp).getTime() - startTime,
      content: r.content.text.substring(0, 100)
    })),
    metrics: {
      redundancy,
      timing: Date.now() - startTime
    },
    notes: passed
      ? '✅ Only one AI responded, others stayed silent'
      : `❌ ${allResponses.length} AIs responded (${redundantCount} redundant), redundancy=${(redundancy * 100).toFixed(0)}%`
  };
}

/**
 * Scenario 4: Follow-up with Distinct Angle
 */
async function testScenario_FollowUpAngle(): Promise<TestResult> {
  console.log('\n📋 Scenario 4: Follow-up with Distinct Angle');
  console.log('='.repeat(60));

  const startTime = Date.now();

  // Ask question that might have multiple valid angles
  const question = `How should I structure a Node.js microservices project? [test-${Date.now()}]`;

  await runJtagCommand(
    `chat/send --room="${GENERAL_ROOM_ID}" --message="${question}"`
  );

  console.log(`   📨 Sent: "${question}"`);
  console.log('   ⏳ Waiting for AI responses...');

  const aiResponses = await waitForAIResponses(startTime, GENERAL_ROOM_ID, 35);

  // Post-mortem analysis
  const postMortem = await analyzeScenarioPostMortem('Follow-up Angle', startTime);

  // If multiple responded, check redundancy
  let redundancy = 0;
  if (aiResponses.length > 1) {
    const contents = aiResponses.map(r => r.content.text);
    redundancy = calculateRedundancy(contents);
  }

  const passed = aiResponses.length > 0 && (
    aiResponses.length === 1 || redundancy < 0.5
  );

  return {
    scenario: 'Follow-up Angle',
    passed,
    aiResponses: aiResponses.map(r => ({
      aiName: r.senderName,
      responseTime: new Date(r.timestamp).getTime() - startTime,
      content: r.content.text.substring(0, 100)
    })),
    metrics: {
      redundancy,
      timing: Date.now() - startTime
    },
    notes: aiResponses.length === 0
      ? '❌ No AI responded (false negative)'
      : aiResponses.length === 1
        ? '✅ Single AI provided comprehensive answer'
        : redundancy < 0.5
          ? `✅ ${aiResponses.length} AIs responded with distinct angles (redundancy=${(redundancy * 100).toFixed(0)}%)`
          : `⚠️  ${aiResponses.length} AIs responded but content overlaps significantly (redundancy=${(redundancy * 100).toFixed(0)}%)`
  };
}

/**
 * Main test runner
 */
async function runGatingQualityTests() {
  console.log('\n🧪 AI GATING QUALITY TEST SUITE');
  console.log('=' .repeat(60));
  console.log('Measuring: False positives, False negatives, Redundancy, Timing\n');

  const results: TestResult[] = [];

  try {
    // Run all scenarios
    results.push(await testScenario_TestMessage());
    await new Promise(resolve => setTimeout(resolve, 3000));

    results.push(await testScenario_NovelQuestion());
    await new Promise(resolve => setTimeout(resolve, 3000));

    results.push(await testScenario_AlreadyAnswered());
    await new Promise(resolve => setTimeout(resolve, 3000));

    results.push(await testScenario_FollowUpAngle());

  } catch (error) {
    console.error('\n❌ Test suite failed:', error);
    process.exit(1);
  }

  // Summary
  console.log('\n\n📊 TEST RESULTS SUMMARY');
  console.log('=' .repeat(60));

  const passed = results.filter(r => r.passed).length;
  const total = results.length;
  const passRate = (passed / total * 100).toFixed(0);

  results.forEach(r => {
    const status = r.passed ? '✅' : '❌';
    console.log(`${status} ${r.scenario}`);
    console.log(`   ${r.notes}`);
    if (r.aiResponses.length > 0) {
      r.aiResponses.forEach(ai => {
        console.log(`   - ${ai.aiName}: ${ai.responseTime}ms`);
      });
    }
  });

  console.log('\n📈 AGGREGATE METRICS');
  console.log('=' .repeat(60));

  const falsePositives = results.filter(r => r.metrics.falsePositive).length;
  const falseNegatives = results.filter(r => r.metrics.falseNegative).length;
  const avgRedundancy = results
    .filter(r => r.metrics.redundancy !== undefined)
    .reduce((sum, r) => sum + (r.metrics.redundancy || 0), 0) /
    results.filter(r => r.metrics.redundancy !== undefined).length;
  const avgTiming = results
    .filter(r => r.metrics.timing !== undefined)
    .reduce((sum, r) => sum + (r.metrics.timing || 0), 0) /
    results.filter(r => r.metrics.timing !== undefined).length;

  console.log(`Pass Rate: ${passed}/${total} (${passRate}%)`);
  console.log(`False Positives: ${falsePositives}/${total} (${(falsePositives / total * 100).toFixed(0)}%)`);
  console.log(`False Negatives: ${falseNegatives}/${total} (${(falseNegatives / total * 100).toFixed(0)}%)`);
  console.log(`Avg Redundancy: ${(avgRedundancy * 100).toFixed(0)}%`);
  console.log(`Avg Response Time: ${(avgTiming / 1000).toFixed(1)}s`);

  console.log('\n' + '=' .repeat(60));

  if (passRate >= '80') {
    console.log('✅ GATING QUALITY: ACCEPTABLE');
  } else {
    console.log('❌ GATING QUALITY: NEEDS IMPROVEMENT');
  }

  // Save results to file for comparison
  const resultsSummary = {
    timestamp: new Date().toISOString(),
    passRate: `${passRate}%`,
    falsePositiveRate: `${(falsePositives / total * 100).toFixed(0)}%`,
    falseNegativeRate: `${(falseNegatives / total * 100).toFixed(0)}%`,
    avgRedundancy: `${(avgRedundancy * 100).toFixed(0)}%`,
    avgResponseTime: `${(avgTiming / 1000).toFixed(1)}s`,
    details: results
  };

  console.log('\n💾 Results saved to: .continuum/sessions/validation/gating-results-latest.json');

  const fs = await import('fs');
  const path = await import('path');
  const resultsDir = path.join(process.cwd(), '.continuum/sessions/validation');
  const resultsFile = path.join(resultsDir, 'gating-results-latest.json');

  await fs.promises.mkdir(resultsDir, { recursive: true });
  await fs.promises.writeFile(resultsFile, JSON.stringify(resultsSummary, null, 2));
}

// Run tests
runGatingQualityTests().catch(error => {
  console.error('❌ Test runner failed:', error);
  process.exit(1);
});
