/**
 * PersonaUser Worker Thread
 * ==========================
 *
 * Minimal skeleton worker that responds to ping-pong.
 * This is Phase 1: Prove threading/IPC works.
 *
 * Once inside this worker context, you're just writing normal code.
 * The hard part is getting messages in and out reliably.
 */

import { parentPort, workerData } from 'worker_threads';

if (!parentPort) {
  throw new Error('This file must be run as a Worker Thread');
}

const personaId = workerData.personaId;

console.log(`🧵 PersonaWorker[${personaId}]: Starting...`);

// Listen for messages from main thread
parentPort.on('message', (msg) => {
  const receiveTime = Date.now();

  console.log(`🧵 PersonaWorker[${personaId}]: Received message type=${msg.type}`);

  if (msg.type === 'ping') {
    // Echo back immediately - prove bidirectional communication works
    parentPort.postMessage({
      type: 'pong',
      timestamp: Date.now(),
      receivedAt: msg.timestamp,
      latency: receiveTime - msg.timestamp
    });

    console.log(`🏓 PersonaWorker[${personaId}]: Pong sent (latency=${receiveTime - msg.timestamp}ms)`);
  }
  else if (msg.type === 'shutdown') {
    console.log(`🛑 PersonaWorker[${personaId}]: Shutdown requested`);
    // Worker will exit naturally when process ends
  }
});

// Signal ready to main thread
parentPort.postMessage({
  type: 'ready',
  personaId: personaId,
  timestamp: Date.now()
});

console.log(`✅ PersonaWorker[${personaId}]: Initialized and ready`);
