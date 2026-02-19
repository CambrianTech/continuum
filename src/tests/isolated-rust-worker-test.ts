/**
 * Isolated Rust Worker Test
 *
 * GOAL: Test the inference worker directly via Unix socket,
 * bypassing all TypeScript abstraction layers to identify
 * where the fundamental TS↔Rust communication breaks down.
 *
 * Run: npx tsx tests/isolated-rust-worker-test.ts
 */

import * as net from 'net';
import * as fs from 'fs';
import * as child_process from 'child_process';

const SOCKET_PATH = '/tmp/jtag-inference.sock';
const TEST_MODEL = 'Qwen/Qwen2-1.5B-Instruct';

// ANSI colors
const GREEN = '\x1b[32m';
const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';

function log(level: 'INFO' | 'PASS' | 'FAIL' | 'WARN', msg: string): void {
  const colors: Record<string, string> = { INFO: CYAN, PASS: GREEN, FAIL: RED, WARN: YELLOW };
  const prefix = colors[level] || '';
  console.log(`${prefix}[${level}]${RESET} ${msg}`);
}

// Check if worker process is alive
function isWorkerAlive(): { alive: boolean; pid?: number } {
  try {
    const result = child_process.execSync('pgrep -f "inference-worker"', { encoding: 'utf8' });
    const pid = parseInt(result.trim().split('\n')[0], 10);
    return { alive: true, pid };
  } catch {
    return { alive: false };
  }
}

// Send raw JSON command and get response
function sendCommand(cmd: object): Promise<{ success: boolean; response?: object; error?: string; raw?: string }> {
  return new Promise((resolve) => {
    const timeout = setTimeout(() => {
      socket.destroy();
      resolve({ success: false, error: 'Timeout (30s)' });
    }, 30000);

    const socket = net.createConnection(SOCKET_PATH);
    let responseData = '';

    socket.on('connect', () => {
      const json = JSON.stringify(cmd) + '\n';
      log('INFO', `Sending: ${json.trim().substring(0, 100)}...`);
      socket.write(json);
    });

    socket.on('data', (data) => {
      responseData += data.toString();
      // Check if we have a complete JSON response (ends with newline)
      if (responseData.includes('\n')) {
        clearTimeout(timeout);
        socket.end();
        try {
          const parsed = JSON.parse(responseData.trim());
          resolve({ success: true, response: parsed, raw: responseData });
        } catch (e) {
          resolve({ success: false, error: `JSON parse error: ${e}`, raw: responseData });
        }
      }
    });

    socket.on('error', (err) => {
      clearTimeout(timeout);
      resolve({ success: false, error: `Socket error: ${err.message}` });
    });

    socket.on('close', () => {
      clearTimeout(timeout);
      if (responseData) {
        try {
          const parsed = JSON.parse(responseData.trim());
          resolve({ success: true, response: parsed, raw: responseData });
        } catch {
          resolve({ success: false, error: 'Connection closed', raw: responseData });
        }
      } else {
        resolve({ success: false, error: 'Connection closed without response' });
      }
    });
  });
}

async function runTests(): Promise<void> {
  console.log('\n' + '='.repeat(70));
  console.log('🔬 ISOLATED RUST WORKER TEST');
  console.log('='.repeat(70) + '\n');

  // Test 1: Check socket exists
  log('INFO', 'Test 1: Socket file exists?');
  if (fs.existsSync(SOCKET_PATH)) {
    log('PASS', `Socket exists: ${SOCKET_PATH}`);
  } else {
    log('FAIL', `Socket not found: ${SOCKET_PATH}`);
    return;
  }

  // Test 2: Check worker process is alive
  log('INFO', 'Test 2: Worker process alive?');
  const workerStatus = isWorkerAlive();
  if (workerStatus.alive) {
    log('PASS', `Worker running (PID: ${workerStatus.pid})`);
  } else {
    log('FAIL', 'Worker process not found');
    return;
  }

  // Test 3: Ping command
  log('INFO', 'Test 3: Ping command');
  const pingResult = await sendCommand({ command: 'ping' });
  if (pingResult.success && (pingResult.response as any)?.success) {
    log('PASS', `Ping response: ${JSON.stringify((pingResult.response as any)?.result)}`);
  } else {
    log('FAIL', `Ping failed: ${pingResult.error || JSON.stringify(pingResult.response)}`);
    return;
  }

  // Check worker still alive after ping
  const afterPing = isWorkerAlive();
  log('INFO', `Worker alive after ping: ${afterPing.alive} (PID: ${afterPing.pid})`);

  // Test 4: Model list
  log('INFO', 'Test 4: List loaded models');
  const listResult = await sendCommand({ command: 'model/list' });
  if (listResult.success) {
    const models = (listResult.response as any)?.result?.models || [];
    log('PASS', `Models loaded: ${models.length === 0 ? 'none' : JSON.stringify(models)}`);
  } else {
    log('FAIL', `List failed: ${listResult.error}`);
  }

  // Test 5: Load model
  log('INFO', `Test 5: Load model (${TEST_MODEL})`);
  log('WARN', 'This may take 30-60 seconds on first load...');
  const loadStart = Date.now();
  const loadResult = await sendCommand({
    command: 'model/load',
    model_id: TEST_MODEL
  });
  const loadTime = Date.now() - loadStart;

  const afterLoad = isWorkerAlive();
  log('INFO', `Worker alive after load attempt: ${afterLoad.alive} (PID: ${afterLoad.pid})`);

  if (loadResult.success && (loadResult.response as any)?.success) {
    const result = (loadResult.response as any)?.result;
    log('PASS', `Model loaded in ${loadTime}ms: ${JSON.stringify(result)}`);
  } else if (loadResult.success && (loadResult.response as any)?.result?.status === 'already_loaded') {
    log('PASS', `Model already loaded`);
  } else {
    log('FAIL', `Load failed: ${loadResult.error || JSON.stringify(loadResult.response)}`);
    // Continue anyway to test other things
  }

  // Test 6: Simple generation (minimal prompt)
  log('INFO', 'Test 6: Minimal generation test');
  const genStart = Date.now();

  const beforeGen = isWorkerAlive();
  log('INFO', `Worker alive before generate: ${beforeGen.alive} (PID: ${beforeGen.pid})`);

  const genResult = await sendCommand({
    command: 'generate',
    model_id: TEST_MODEL,
    prompt: 'Say "hello" and nothing else.',
    max_tokens: 10,
    temperature: 0.1
  });
  const genTime = Date.now() - genStart;

  const afterGen = isWorkerAlive();
  log('INFO', `Worker alive after generate: ${afterGen.alive} (PID: ${afterGen.pid})`);

  if (!afterGen.alive && beforeGen.alive) {
    log('FAIL', '🚨 WORKER CRASHED DURING GENERATION!');

    // Check for any crash info
    try {
      const logs = child_process.execSync(
        'tail -50 .continuum/jtag/logs/system/rust-worker.log 2>/dev/null || echo "No log"',
        { encoding: 'utf8', cwd: '/Volumes/FlashGordon/cambrian/continuum/src' }
      );
      log('INFO', 'Last worker logs:\n' + logs);
    } catch {
      log('WARN', 'Could not read worker logs');
    }
  } else if (genResult.success && (genResult.response as any)?.success) {
    const result = (genResult.response as any)?.result;
    log('PASS', `Generated in ${genTime}ms: "${result?.text?.substring(0, 50)}..."`);
    log('INFO', `Tokens: prompt=${result?.prompt_tokens}, generated=${result?.generated_tokens}`);
  } else {
    log('FAIL', `Generation failed: ${genResult.error || JSON.stringify(genResult.response)}`);
  }

  // Test 7: Larger generation (stress test)
  if (isWorkerAlive().alive) {
    log('INFO', 'Test 7: Larger generation (50 tokens)');
    const stress1 = await sendCommand({
      command: 'generate',
      model_id: TEST_MODEL,
      prompt: 'Write a short poem about rust programming:',
      max_tokens: 50,
      temperature: 0.7
    });

    const afterStress1 = isWorkerAlive();
    if (!afterStress1.alive) {
      log('FAIL', '🚨 WORKER CRASHED ON LARGER GENERATION!');
    } else if (stress1.success && (stress1.response as any)?.success) {
      log('PASS', `Larger generation succeeded`);
    } else {
      log('FAIL', `Larger generation failed: ${stress1.error || JSON.stringify(stress1.response)}`);
    }
  }

  // Test 8: Concurrent connections (race condition test)
  if (isWorkerAlive().alive) {
    log('INFO', 'Test 8: Concurrent ping while model loaded');
    const concurrentPings = await Promise.all([
      sendCommand({ command: 'ping' }),
      sendCommand({ command: 'ping' }),
      sendCommand({ command: 'model/list' }),
    ]);

    const allPassed = concurrentPings.every(r => r.success && (r.response as any)?.success);
    if (allPassed) {
      log('PASS', 'Concurrent connections handled correctly');
    } else {
      log('FAIL', 'Some concurrent requests failed');
    }
  }

  // Final status
  console.log('\n' + '='.repeat(70));
  const finalStatus = isWorkerAlive();
  if (finalStatus.alive) {
    log('PASS', `✅ All tests completed. Worker still alive (PID: ${finalStatus.pid})`);
  } else {
    log('FAIL', '❌ Worker died during testing!');
  }
  console.log('='.repeat(70) + '\n');
}

runTests().catch(console.error);
