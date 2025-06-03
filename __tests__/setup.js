/**
 * Jest setup file
 */

// Suppress console.log during tests unless specifically needed
global.console = {
  ...console,
  log: process.env.NODE_ENV === 'test' && !process.env.VERBOSE_TESTS ? jest.fn() : console.log,
  error: console.error,
  warn: console.warn,
  info: console.info,
  debug: console.debug,
};

// Mock environment variables
process.env.NODE_ENV = 'test';
process.env.ANTHROPIC_API_KEY = 'test-anthropic-key';
process.env.OPENAI_API_KEY = 'test-openai-key';

// Global test utilities
global.mockAIResponse = (text, usage = { input_tokens: 50, output_tokens: 25 }) => ({
  content: [{ text }],
  usage
});

global.mockWebSocketMessage = (type, data) => ({
  type,
  ...data
});

// Increase timeout for integration tests
jest.setTimeout(10000);