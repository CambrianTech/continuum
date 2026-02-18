/**
 * Generic AI Worker - Dumb Router
 * Just forwards requests to AI provider and returns raw responses
 * NO business logic, NO parsing, NO decisions
 */
import { parentPort, workerData, threadId } from 'worker_threads';

if (!parentPort) throw new Error('Must run as Worker Thread');

const { providerType = 'mock', providerConfig = {} } = workerData;

console.log(`🧵 [WORKER-${threadId}] AI Worker starting (provider: ${providerType})`);

// Minimal Ollama API call - just forward request, return response
async function callOllama(prompt, model, temperature, maxTokens) {
  const apiEndpoint = providerConfig.apiEndpoint || 'http://localhost:11434';
  const response = await fetch(`${apiEndpoint}/api/generate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: model,
      prompt: prompt,
      temperature: temperature,
      num_predict: maxTokens,
      stream: false
    })
  });

  if (!response.ok) {
    throw new Error(`Ollama API error: ${response.status} ${response.statusText}`);
  }

  const result = await response.json();
  return result.response; // Raw text from Ollama
}

// Mock provider - simulates delay and returns fake AI response
async function callMock(prompt, model, temperature, maxTokens) {
  const thinkTime = 100 + Math.random() * 400;
  await new Promise(r => setTimeout(r, thinkTime));

  // Simulate varied AI responses (dumb router just returns text)
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
      // Generic AI text generation - just forward to provider
      const startTime = Date.now();

      try {
        let responseText;

        if (providerType === 'ollama') {
          responseText = await callOllama(
            msg.prompt,
            msg.model || providerConfig.model || 'llama3.2:1b',
            msg.temperature || 0.7,
            msg.maxTokens || 200
          );
        } else {
          // Mock fallback
          responseText = await callMock(
            msg.prompt,
            msg.model,
            msg.temperature,
            msg.maxTokens
          );
        }

        // Return raw response - caller does ALL parsing/logic
        parentPort.postMessage({
          type: 'result',
          requestId: msg.requestId,
          timestamp: Date.now(),
          data: {
            text: responseText,              // Raw text from provider
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

  console.log(`✅ [WORKER-${threadId}] AI Worker ready`);
  parentPort.postMessage({ type: 'ready', timestamp: Date.now() });
})();
