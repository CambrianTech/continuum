/**
 * Test SignalDetector - Content-based training signal classification
 *
 * The SignalDetector uses AI to classify messages as training signals.
 * It focuses on MESSAGE CONTENT, not sender type.
 */

import { SignalDetector } from '../../system/user/server/modules/SignalDetector';

const detector = new SignalDetector();

// Mock messages - note: senderType doesn't affect classification anymore
const mockMessage = (text: string, senderType: string = 'human'): any => ({
  id: 'test-id',
  roomId: 'test-room',
  senderId: 'sender-id',
  senderName: 'Test User',
  senderType,
  content: { text, media: [] },
  timestamp: new Date().toISOString(),
});

const mockAIResponse = (text: string): any => ({
  ...mockMessage(text, 'persona'),
  id: 'ai-msg-id',
  senderId: 'ai-id',
  senderName: 'Helper AI',
});

// Test correction patterns (synchronous - quick heuristics)
console.log('\n=== Testing Correction Patterns (Sync) ===');
const corrections = [
  "No, that's not what I meant",
  "Wrong, the answer is 42",
  "That's not correct",
  "Incorrect - try again"
];

for (const text of corrections) {
  const signal = detector.detectSignal(mockMessage(text), mockAIResponse("Here's my response"), []);
  const result = signal ? `${signal.type}/${signal.trait} (${signal.confidence})` : 'NO SIGNAL';
  console.log(`"${text.slice(0, 40)}..." => ${result}`);
}

// Test approval patterns
console.log('\n=== Testing Approval Patterns (Sync) ===');
const approvals = [
  "Perfect!",
  "Exactly!",
  "Thanks!",
  "Great!"
];

for (const text of approvals) {
  const signal = detector.detectSignal(mockMessage(text), mockAIResponse("Here's my response"), []);
  const result = signal ? `${signal.type}/${signal.polarity} (${signal.confidence})` : 'NO SIGNAL';
  console.log(`"${text}" => ${result}`);
}

// Test explicit feedback
console.log('\n=== Testing Explicit Feedback Patterns (Sync) ===');
const feedback = [
  "Be more concise please",
  "That's too long",
  "Be more detailed"
];

for (const text of feedback) {
  const signal = detector.detectSignal(mockMessage(text), mockAIResponse("Here's my response"), []);
  const result = signal ? `${signal.type}/${signal.trait} (${signal.confidence})` : 'NO SIGNAL';
  console.log(`"${text}" => ${result}`);
}

// Test frustration patterns
console.log('\n=== Testing Frustration Patterns (Sync) ===');
const frustration = [
  "I already said that",
  "Again: please use Python",
  "How many times do I have to ask?"
];

for (const text of frustration) {
  const signal = detector.detectSignal(mockMessage(text), mockAIResponse("Here's my response"), []);
  const result = signal ? `${signal.type}/${signal.trait} (${signal.confidence})` : 'NO SIGNAL';
  console.log(`"${text}" => ${result}`);
}

// Test normal messages (should NOT be signals)
console.log('\n=== Testing Normal Messages (Should NOT be signals) ===');
const normalMessages = [
  "Can you help me with Python?",
  "What's the weather like?",
  "Let me think about that",
  "Here's my code: function foo() {}"
];

for (const text of normalMessages) {
  const signal = detector.detectSignal(mockMessage(text), mockAIResponse("Here's my response"), []);
  const result = signal ? `UNEXPECTED: ${signal.type}/${signal.trait}` : 'NO SIGNAL ✓';
  console.log(`"${text.slice(0, 40)}..." => ${result}`);
}

// Test that senderType doesn't affect classification
console.log('\n=== Testing Content-Based (senderType Ignored) ===');
const senderTypes = ['human', 'agent', 'persona', 'system'];
for (const senderType of senderTypes) {
  const signal = detector.detectSignal(
    mockMessage("Perfect!", senderType),
    mockAIResponse("Here's my response"),
    []
  );
  const result = signal ? `${signal.type}/${signal.polarity}` : 'NO SIGNAL';
  console.log(`senderType="${senderType}" + "Perfect!" => ${result}`);
}

console.log('\n✅ Signal detector tests complete!');
console.log('\nNote: Async AI classification (detectSignalAsync) requires running system with Candle.');
