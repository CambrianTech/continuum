/**
 * PersonaUser Worker Thread - Minimal standalone JS worker
 * Supports mock and real Ollama inference without TS dependencies
 */
import { parentPort, workerData, threadId } from 'worker_threads';

if (!parentPort) throw new Error('Must run as Worker Thread');

const { personaId, providerType = 'mock', providerConfig = {} } = workerData;

// PROOF: Log worker thread ID (different from main thread)
console.log(`🧵 [WORKER-${threadId}] PersonaWorker[${personaId}]: Starting in worker thread ${threadId}`);

// Minimal Ollama API call (no dependencies)
async function callOllama(prompt, model, temperature = 0.7, maxTokens = 200) {
  const apiEndpoint = providerConfig.apiEndpoint || 'http://localhost:11434';
  const response = await fetch(`${apiEndpoint}/api/generate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: model || 'llama3.2:1b',
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
  return result.response; // Extract text from Ollama response
}

(async () => {
  parentPort.on('message', async (msg) => {
    if (msg.type === 'ping') {
      console.log(`🏓 [WORKER-${threadId}] Received ping, sending pong from worker thread ${threadId}`);
      parentPort.postMessage({
        type: 'pong',
        timestamp: Date.now(),
        receivedAt: msg.timestamp,
        latency: Date.now() - msg.timestamp
      });
    }
    else if (msg.type === 'evaluate') {
      console.log(`🤔 [WORKER-${threadId}] Evaluating message ${msg.message.id} in worker thread ${threadId}`);
      const startTime = Date.now();
      let confidence, shouldRespond, reasoning, processingTime;

      try {
        if (providerType === 'ollama') {
          // Real Ollama inference (minimal HTTP call)
          console.log(`🧠 [WORKER-${threadId}] Calling Ollama API from worker thread ${threadId}...`);
          const prompt = `Evaluate if you should respond to this message.

Message: "${msg.message.content}"
Sender: ${msg.message.senderId}

Respond with:
CONFIDENCE: <number between 0.0 and 1.0>
REASONING: <brief explanation>`;

          const model = providerConfig.model || 'llama3.2:1b';
          const responseText = await callOllama(prompt, model, 0.7, 200);
          console.log(`✅ [WORKER-${threadId}] Ollama responded in ${Date.now() - startTime}ms from worker thread ${threadId}`);

          // Parse confidence from AI response (just return data, don't decide)
          const confMatch = responseText.match(/CONFIDENCE:\s*([0-9.]+)/i);
          confidence = confMatch ? parseFloat(confMatch[1]) : 0.5;
          confidence = Math.max(0, Math.min(1, confidence)); // Clamp 0-1
          reasoning = responseText.substring(0, 200);
          processingTime = Date.now() - startTime;

          // Worker is pure computation - PersonaUser decides shouldRespond
        } else {
          // Mock evaluation (Phase 2 fallback)
          const thinkTime = 100 + Math.random() * 400;
          await new Promise(r => setTimeout(r, thinkTime));
          const content = msg.message.content.toLowerCase();
          confidence = 0.3 + Math.random() * 0.6;
          if (content.includes('test')) confidence *= 0.3;
          if (content.includes('?')) confidence = Math.min(confidence * 1.3, 0.95);
          reasoning = `Mock (${thinkTime.toFixed(0)}ms)`;
          processingTime = Date.now() - startTime;
        }

        // Return evaluation data only - PersonaUser decides shouldRespond based on threshold
        parentPort.postMessage({
          type: 'result',
          timestamp: Date.now(),
          data: {
            messageId: msg.message.id,
            confidence,    // 0.0-1.0 score
            reasoning,     // Why this confidence level
            processingTime // Milliseconds to evaluate
            // NO shouldRespond - that's business logic, not computation
          }
        });
      } catch (error) {
        parentPort.postMessage({
          type: 'error',
          timestamp: Date.now(),
          data: {
            messageId: msg.message.id,
            error: error.message
          }
        });
      }
    }
  });

  console.log(`✅ [WORKER-${threadId}] PersonaWorker[${personaId}]: Ready in worker thread ${threadId}`);
  parentPort.postMessage({ type: 'ready', personaId, timestamp: Date.now() });
})();
