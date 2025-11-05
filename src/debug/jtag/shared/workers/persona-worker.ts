/**
 * PersonaUser Worker Thread
 * ==========================
 *
 * Worker thread for persona evaluation.
 * Supports both mock (Phase 2) and real inference (Phase 3+).
 *
 * Phase 1: Skeleton (ping-pong)
 * Phase 2: Mock evaluation
 * Phase 3: Real Ollama inference
 */

import { parentPort, workerData } from 'worker_threads';
import { OllamaAdapter } from '../../daemons/ai-provider-daemon/adapters/ollama/shared/OllamaAdapter';
import type { BaseAIProviderAdapter } from '../../daemons/ai-provider-daemon/shared/BaseAIProviderAdapter';

if (!parentPort) {
  throw new Error('This file must be run as a Worker Thread');
}

const personaId: string = workerData.personaId;
const providerType: string = workerData.providerType || 'mock';
const providerConfig: Record<string, unknown> = workerData.providerConfig || {};

console.log(`🧵 PersonaWorker[${personaId}]: Starting...`);
console.log(`🧵 PersonaWorker[${personaId}]: Provider type: ${providerType}`);

// Initialize provider (if not mock)
let provider: BaseAIProviderAdapter | null = null;

async function initializeProvider(): Promise<void> {
  if (providerType === 'ollama') {
    console.log(`🧵 PersonaWorker[${personaId}]: Initializing OllamaAdapter...`);

    const adapter = new OllamaAdapter({
      apiEndpoint: (providerConfig.apiEndpoint as string) || 'http://localhost:11434',
      defaultModel: (providerConfig.model as string) || 'llama3.2:1b'
    });
    await adapter.initialize();
    provider = adapter;
    console.log(`✅ PersonaWorker[${personaId}]: OllamaAdapter initialized`);
  }
}

// Main async initialization
(async () => {
  // Initialize provider before signaling ready
  await initializeProvider();

  // Listen for messages from main thread
  parentPort!.on('message', async (msg) => {
    const receiveTime = Date.now();

    console.log(`🧵 PersonaWorker[${personaId}]: Received message type=${msg.type}`);

    if (msg.type === 'ping') {
      // Echo back immediately - prove bidirectional communication works
      parentPort!.postMessage({
        type: 'pong',
        timestamp: Date.now(),
        receivedAt: msg.timestamp,
        latency: receiveTime - msg.timestamp
      });

      console.log(`🏓 PersonaWorker[${personaId}]: Pong sent (latency=${receiveTime - msg.timestamp}ms)`);
    }
    else if (msg.type === 'evaluate') {
      const startTime = Date.now();
      console.log(`🤔 PersonaWorker[${personaId}]: Evaluating message ${msg.message.id}`);

      let confidence = 0;
      let shouldRespond = false;
      let reasoning = '';
      let processingTime = 0;

      try {
        if (provider) {
          // Real Ollama inference (Phase 3)
          console.log(`🧠 PersonaWorker[${personaId}]: Using real Ollama inference...`);

          const prompt = `You are evaluating whether you should respond to a message in a conversation.

Message: "${msg.message.content}"
Sender: ${msg.message.senderId}

Respond with a confidence score (0.0-1.0) indicating whether you should respond.
Consider:
- Is this message directed at you or relevant to your expertise?
- Is it a test message that should be ignored?
- Would your response add value to the conversation?

Format your response as:
CONFIDENCE: <number between 0.0 and 1.0>
REASONING: <brief explanation>`;

          const result = await provider.generateText({
            messages: [
              { role: 'user', content: prompt }
            ],
            model: (providerConfig.model as string) || 'llama3.2:1b',
            temperature: 0.7,
            maxTokens: 200
          });

        // Parse confidence from AI response
        const confidenceMatch = result.text.match(/CONFIDENCE:\s*([0-9.]+)/i);
        const reasoningMatch = result.text.match(/REASONING:\s*(.+)/is);

        confidence = confidenceMatch ? parseFloat(confidenceMatch[1]) : 0.5;
        confidence = Math.max(0, Math.min(1, confidence)); // Clamp 0-1
        shouldRespond = confidence > 0.5;
        reasoning = reasoningMatch ? reasoningMatch[1].trim().substring(0, 200) : result.text.substring(0, 200);

        processingTime = Date.now() - startTime;
        console.log(`✅ PersonaWorker[${personaId}]: Real inference complete - conf=${confidence.toFixed(2)}, took ${processingTime}ms`);

      } else {
        // Mock evaluation (Phase 2 - fallback)
        console.log(`🎭 PersonaWorker[${personaId}]: Using mock evaluation...`);

        const thinkTime = 100 + Math.random() * 400;
        await new Promise(resolve => setTimeout(resolve, thinkTime));

        const content = msg.message.content.toLowerCase();
        confidence = 0.3 + Math.random() * 0.6;

        if (content.includes('test') || msg.message.senderId.includes('test')) {
          confidence *= 0.3;
        }
        if (content.includes('?') || content.includes('what') || content.includes('how') || content.includes('explain')) {
          confidence *= 1.3;
          confidence = Math.min(confidence, 0.95);
        }
        if (content.match(/^(hi|hello|hey|goodbye|bye)$/)) {
          confidence = 0.5 + Math.random() * 0.2;
        }

        shouldRespond = confidence > 0.5;
        processingTime = Date.now() - startTime;
        reasoning = `Mock evaluation (${thinkTime.toFixed(0)}ms think time, conf=${confidence.toFixed(2)})`;
      }

      // Send result back to main thread
      parentPort!.postMessage({
        type: 'result',
        timestamp: Date.now(),
        data: {
          messageId: msg.message.id,
          confidence: confidence,
          shouldRespond: shouldRespond,
          reasoning: reasoning,
          processingTime: processingTime
        }
      });

      console.log(`✅ PersonaWorker[${personaId}]: Evaluated ${msg.message.id} - conf=${confidence.toFixed(2)}, respond=${shouldRespond}, took ${processingTime}ms`);

    } catch (error) {
      // Send error back to main thread
      console.error(`❌ PersonaWorker[${personaId}]: Evaluation failed:`, error);
      parentPort!.postMessage({
        type: 'error',
        timestamp: Date.now(),
        data: {
          messageId: msg.message.id,
          error: error instanceof Error ? error.message : String(error)
        }
      });
    }
  }
  else if (msg.type === 'shutdown') {
    console.log(`🛑 PersonaWorker[${personaId}]: Shutdown requested`);
    // Worker will exit naturally when process ends
  }
  });

  // Signal ready to main thread
  parentPort!.postMessage({
    type: 'ready',
    personaId: personaId,
    timestamp: Date.now()
  });

  console.log(`✅ PersonaWorker[${personaId}]: Initialized and ready`);
})().catch((error) => {
  console.error(`❌ PersonaWorker[${personaId}]: Initialization failed:`, error);
  process.exit(1);
});
