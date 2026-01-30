/**
 * PersonaUser Worker Thread
 * ==========================
 *
 * Worker thread for persona evaluation.
 * Supports both mock (Phase 2) and real inference (Phase 3+).
 *
 * Phase 1: Skeleton (ping-pong)
 * Phase 2: Mock evaluation
 * Phase 3: Real Candle (native Rust) inference
 *
 * NOTE: Ollama is REMOVED. Candle is the ONLY local inference path.
 */

import { parentPort, workerData } from 'worker_threads';
import { CandleGrpcAdapter } from '../../daemons/ai-provider-daemon/adapters/candle-grpc/shared/CandleGrpcAdapter';
import type { BaseAIProviderAdapter } from '../../daemons/ai-provider-daemon/shared/BaseAIProviderAdapter';

if (!parentPort) {
  throw new Error('This file must be run as a Worker Thread');
}

const personaId: string = workerData.personaId;
const providerType: string = workerData.providerType || 'mock';
const _providerConfig: Record<string, unknown> = workerData.providerConfig || {};

console.log(`🧵 PersonaWorker[${personaId}]: Starting...`);
console.log(`🧵 PersonaWorker[${personaId}]: Provider type: ${providerType}`);

// Initialize provider (if not mock)
let provider: BaseAIProviderAdapter | null = null;

async function initializeProvider(): Promise<void> {
  // 'candle' or legacy 'ollama' both use Candle now
  if (providerType === 'candle' || providerType === 'ollama' || providerType === 'local') {
    console.log(`🧵 PersonaWorker[${personaId}]: Initializing CandleGrpcAdapter...`);

    const adapter = new CandleGrpcAdapter();
    await adapter.initialize();
    provider = adapter;
    console.log(`✅ PersonaWorker[${personaId}]: CandleGrpcAdapter initialized`);
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
            model: (_providerConfig.model as string) || 'llama3.2:1b',
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
        // Smart heuristics evaluation with PersonaState integration
        console.log(`🎭 PersonaWorker[${personaId}]: Using smart heuristics with state...`);

        const thinkTime = 100 + Math.random() * 400;
        await new Promise(resolve => setTimeout(resolve, thinkTime));

        const content = msg.message.content.toLowerCase();
        const state = msg.personaState || { energy: 0.8, attention: 0.7, mood: 'active' };
        const config = msg.config || { responseThreshold: 50, temperature: 0.7 };

        // Base confidence from content analysis
        confidence = 0.3 + Math.random() * 0.6;

        // Content-based modifiers
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

        // State-based modifiers (energy, attention, mood)
        // Low energy → less likely to respond (except high-priority)
        if (state.energy < 0.3) {
          confidence *= 0.5;  // 50% penalty when exhausted
        } else if (state.energy < 0.6) {
          confidence *= 0.8;  // 20% penalty when tired
        }

        // Low attention → less likely to respond
        if (state.attention < 0.4) {
          confidence *= 0.7;  // 30% penalty when distracted
        }

        // Mood affects baseline engagement
        if (state.mood === 'overwhelmed') {
          confidence *= 0.4;  // 60% penalty when overwhelmed
        } else if (state.mood === 'tired') {
          confidence *= 0.7;  // 30% penalty when tired
        } else if (state.mood === 'active') {
          confidence *= 1.1;  // 10% boost when active
        }

        // Temperature affects randomness/engagement
        // High temperature → more willing to respond (more random)
        // Low temperature → more selective (deterministic)
        if (config.temperature > 0.8) {
          confidence += (Math.random() - 0.5) * 0.3;  // ±15% randomness
        } else if (config.temperature < 0.3) {
          // Low temp → more deterministic, boost only if clearly relevant
          if (confidence < 0.6) {
            confidence *= 0.8;  // 20% penalty for marginal messages
          }
        }

        // Clamp final confidence to [0, 1]
        confidence = Math.max(0, Math.min(1, confidence));
        shouldRespond = confidence > 0.5;
        processingTime = Date.now() - startTime;

        reasoning = `Smart heuristics: energy=${state.energy.toFixed(2)}, attention=${state.attention.toFixed(2)}, mood=${state.mood}, temp=${config.temperature.toFixed(2)}, conf=${confidence.toFixed(2)}`;
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
