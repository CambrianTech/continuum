/**
 * Generic AI Worker - Dumb Router
 * Just forwards requests to AI provider and returns raw responses
 * NO business logic, NO parsing, NO decisions
 *
 * NOTE: Legacy file — inference now goes through Candle (Rust) or cloud adapters.
 * Kept for mock/test purposes only.
 */
import { parentPort, workerData, threadId } from 'worker_threads';

if (!parentPort) throw new Error('Must run as Worker Thread');

const { providerType = 'mock', providerConfig = {} } = workerData;

// Mock provider - simulates delay and returns fake AI response
async function callMock(prompt, model, temperature, maxTokens) {
  const thinkTime = 100 + Math.random() * 400;
  await new Promise(r => setTimeout(r, thinkTime));

  const confidence = 0.3 + Math.random() * 0.6;
  return `CONFIDENCE: ${confidence.toFixed(2)}\nREASONING: Mock AI response after ${thinkTime.toFixed(0)}ms`;
}

(async () => {
  parentPort.on('message', async (msg) => {
    if (msg.type === 'ping') {
      parentPort.postMessage({
        type: 'pong',
        timestamp: Date.now(),
        receivedAt: msg.timestamp
      });
    }
    else if (msg.type === 'generate') {
      const startTime = Date.now();

      try {
        const responseText = await callMock(
          msg.prompt,
          msg.model,
          msg.temperature,
          msg.maxTokens
        );

        parentPort.postMessage({
          type: 'result',
          requestId: msg.requestId,
          timestamp: Date.now(),
          data: {
            text: responseText,
            processingTime: Date.now() - startTime
          }
        });
      } catch (error) {
        parentPort.postMessage({
          type: 'error',
          requestId: msg.requestId,
          timestamp: Date.now(),
          error: error.message
        });
      }
    }
  });

  parentPort.postMessage({ type: 'ready', timestamp: Date.now() });
})();
