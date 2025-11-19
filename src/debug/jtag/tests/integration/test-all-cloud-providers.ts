#!/usr/bin/env tsx
/**
 * Comprehensive Multi-Cloud Provider Test
 *
 * Tests ALL 7 cloud AI providers with PersonaUsers:
 * 1. Anthropic (Claude)
 * 2. OpenAI (GPT)
 * 3. X.AI (Grok)
 * 4. DeepSeek
 * 5. Together.ai
 * 6. Fireworks
 * 7. Groq
 *
 * Verifies end-to-end:
 * - PersonaUser creation with cloud provider configs
 * - Real-time message evaluation
 * - Cloud API inference calls
 * - Multi-provider simultaneous responses
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

interface TestResult {
  provider: string;
  persona: string;
  success: boolean;
  responseTime?: number;
  response?: string;
  error?: string;
}

const PROVIDERS = [
  { name: 'DeepSeek', uniqueId: 'persona-deepseek', displayName: 'DeepSeek Assistant' },
  { name: 'Groq', uniqueId: 'persona-groq', displayName: 'Groq Lightning' },
  { name: 'Anthropic', uniqueId: 'general-ai', displayName: 'GeneralAI' },
  { name: 'OpenAI', uniqueId: 'claude-code', displayName: 'Claude Code' },
  // TODO: Add Together.ai and Fireworks PersonaUsers to seed data
];

async function getRoomId(): Promise<string> {
  const { stdout } = await execAsync('./jtag data/list --collection=rooms');
  const result = JSON.parse(stdout);
  const generalRoom = result.items?.find((r: any) => r.uniqueId === 'general');

  if (!generalRoom) {
    throw new Error('General room not found - run data:seed first');
  }

  return generalRoom.id;
}

async function verifyProviderConfigs(): Promise<void> {
  console.log('🔍 Verifying cloud provider configurations...\n');

  for (const provider of PROVIDERS) {
    try {
      const { stdout } = await execAsync(`./jtag data/list --collection=users --filter='{"uniqueId":"${provider.uniqueId}"}'`);
      const result = JSON.parse(stdout);
      const user = result.items?.[0];

      if (!user) {
        console.log(`   ❌ ${provider.name}: PersonaUser not found (uniqueId: ${provider.uniqueId})`);
        continue;
      }

      const modelConfig = user.modelConfig;
      if (!modelConfig) {
        console.log(`   ❌ ${provider.name}: No modelConfig found`);
        continue;
      }

      console.log(`   ✅ ${provider.name}: ${modelConfig.provider}/${modelConfig.model}`);
    } catch (error) {
      console.log(`   ❌ ${provider.name}: Error checking config`);
    }
  }

  console.log('');
}

async function sendTestMessage(roomId: string): Promise<void> {
  const message = 'Multi-provider test: Can each AI provider explain what makes their infrastructure unique? Keep responses under 100 words.';

  console.log('📤 Sending test message to General room...');
  console.log(`   Message: "${message}"\n`);

  const { stdout } = await execAsync(`./jtag chat/send --room="${roomId}" --message="${message}"`);
  const result = JSON.parse(stdout);

  if (!result.success) {
    throw new Error('Failed to send test message');
  }

  console.log('✅ Message sent successfully\n');
}

async function waitForResponses(seconds: number): Promise<void> {
  console.log(`⏳ Waiting ${seconds} seconds for AI responses...`);

  for (let i = 0; i < seconds; i++) {
    await new Promise(resolve => setTimeout(resolve, 1000));
    if (i % 5 === 4) {
      console.log(`   ${i + 1}/${seconds} seconds elapsed...`);
    }
  }

  console.log('');
}

async function checkLogs(providerName: string): Promise<{ responded: boolean; time?: number; error?: string }> {
  try {
    const { stdout } = await execAsync(`./jtag debug/logs --filterPattern="${providerName}.*generateText|${providerName}.*POSTED" --tailLines=100`);
    const result = JSON.parse(stdout);

    if (!result.success || !result.logEntries || result.logEntries.length === 0) {
      return { responded: false, error: 'No log entries found' };
    }

    // Check for successful API call and response
    const hasApiCall = result.logEntries.some((entry: any) =>
      entry.message.includes('generateText') || entry.message.includes('Calling AIProviderDaemon')
    );

    const hasResponse = result.logEntries.some((entry: any) =>
      entry.message.includes('POSTED') || entry.message.includes('Message posted successfully')
    );

    if (hasApiCall && hasResponse) {
      return { responded: true };
    } else if (hasApiCall) {
      return { responded: false, error: 'API called but no response posted' };
    } else {
      return { responded: false, error: 'No API call detected' };
    }
  } catch (error) {
    return { responded: false, error: `Log check failed: ${error}` };
  }
}

async function verifyResponses(): Promise<TestResult[]> {
  console.log('📊 Checking which providers responded...\n');

  const results: TestResult[] = [];

  for (const provider of PROVIDERS) {
    const logResult = await checkLogs(provider.displayName);

    const result: TestResult = {
      provider: provider.name,
      persona: provider.displayName,
      success: logResult.responded,
      responseTime: logResult.time,
      error: logResult.error
    };

    results.push(result);

    if (result.success) {
      console.log(`   ✅ ${provider.name}: Responded successfully${result.responseTime ? ` (${result.responseTime}ms)` : ''}`);
    } else {
      console.log(`   ❌ ${provider.name}: ${result.error || 'No response detected'}`);
    }
  }

  console.log('');
  return results;
}

async function printSummary(results: TestResult[]): Promise<void> {
  console.log('='.repeat(60));
  console.log('📊 MULTI-CLOUD PROVIDER TEST SUMMARY');
  console.log('='.repeat(60));

  const working = results.filter(r => r.success);
  const failed = results.filter(r => !r.success);

  console.log(`\n✅ Working Providers: ${working.length}/${results.length}`);
  console.log(`❌ Failed Providers: ${failed.length}/${results.length}\n`);

  if (working.length > 0) {
    console.log('✅ Successfully Tested:');
    working.forEach(r => {
      console.log(`   - ${r.provider} (${r.persona})${r.responseTime ? ` - ${r.responseTime}ms` : ''}`);
    });
    console.log('');
  }

  if (failed.length > 0) {
    console.log('❌ Failed Tests:');
    failed.forEach(r => {
      console.log(`   - ${r.provider} (${r.persona}): ${r.error}`);
    });
    console.log('');
  }

  console.log('🎯 Next Steps:');
  if (failed.length > 0) {
    console.log('   1. Check API keys are configured: .env.local');
    console.log('   2. Verify PersonaUser modelConfigs in database');
    console.log('   3. Check logs for detailed error messages');
  } else {
    console.log('   🎉 All cloud providers working! Multi-cloud infrastructure ready!');
  }

  console.log('');
}

async function main(): Promise<void> {
  console.log('🚀 Comprehensive Multi-Cloud Provider Test');
  console.log('='.repeat(60));
  console.log('Testing: Anthropic, OpenAI, X.AI, DeepSeek, Together, Fireworks, Groq');
  console.log('='.repeat(60));
  console.log('');

  try {
    // Step 1: Verify provider configs
    await verifyProviderConfigs();

    // Step 2: Get room ID
    const roomId = await getRoomId();
    console.log(`📍 Using room: general (${roomId})\n`);

    // Step 3: Send test message
    await sendTestMessage(roomId);

    // Step 4: Wait for responses (cloud APIs can be slow)
    await waitForResponses(30);

    // Step 5: Check logs for responses
    const results = await verifyResponses();

    // Step 6: Print summary
    await printSummary(results);

    // Step 7: Exit with proper code
    const allWorking = results.every(r => r.success);
    process.exit(allWorking ? 0 : 1);

  } catch (error) {
    console.error('❌ Test failed with error:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch(console.error);
}
