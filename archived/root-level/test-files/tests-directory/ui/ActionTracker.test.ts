/**
 * Unit Tests for ActionTracker
 */
import { ActionTracker, ActionStatus } from '../../src/ui/ActionTracker.js';

// Mock DOM
const mockElement = {
  innerHTML: ''
};

const mockGetElementById = jest.fn(() => mockElement);

global.document = {
  getElementById: mockGetElementById
} as any;

describe('ActionTracker', () => {
  let actionTracker: ActionTracker;

  beforeEach(() => {
    jest.clearAllMocks();
    mockElement.innerHTML = '';
    actionTracker = new ActionTracker();
  });

  describe('updateAction', () => {
    it('should format action with italic emphasis', () => {
      actionTracker.updateAction('Thinking...');
      expect(mockElement.innerHTML).toBe('<em>Thinking...</em>');
    });

    it('should clear action when empty string provided', () => {
      actionTracker.updateAction('');
      expect(mockElement.innerHTML).toBe('');
    });
  });

  describe('clearAction', () => {
    it('should clear the action status', () => {
      actionTracker.updateAction('Working...');
      actionTracker.clearAction();
      expect(mockElement.innerHTML).toBe('');
    });
  });

  describe('parseWorkingMessage - Agent Detection', () => {
    it('should detect PlannerAI from message', () => {
      const result = actionTracker.parseWorkingMessage('PlannerAI processing: analyze this task');
      expect(result.agent).toBe('PlannerAI');
      expect(result.action).toBe('PlannerAI is thinking...');
    });

    it('should detect CodeAI from message', () => {
      const result = actionTracker.parseWorkingMessage('CodeAI processing: implement the fix');
      expect(result.agent).toBe('CodeAI');
      expect(result.action).toBe('CodeAI is thinking...');
    });

    it('should default to AI for unknown agents', () => {
      const result = actionTracker.parseWorkingMessage('processing: some task');
      expect(result.agent).toBe('AI');
      expect(result.action).toBe('AI is thinking...');
    });
  });

  describe('parseWorkingMessage - Routing Actions', () => {
    it('should detect intelligent routing phase', () => {
      const result = actionTracker.parseWorkingMessage('Enhanced intelligent routing: complex task');
      expect(result.action).toBe('Analyzing request and selecting best AI...');
    });

    it('should detect strategic task routing', () => {
      const result = actionTracker.parseWorkingMessage('Strategic/complex task - routing to PlannerAI');
      expect(result.action).toBe('Routing to strategic AI...');
    });

    it('should detect coordination requirement', () => {
      const result = actionTracker.parseWorkingMessage('COORDINATION DETECTED - multi-AI required');
      expect(result.action).toBe('Multi-AI coordination required...');
    });

    it('should detect coordination steps', () => {
      const result1 = actionTracker.parseWorkingMessage('Step 1: Sending to PlannerAI');
      const result2 = actionTracker.parseWorkingMessage('Step 2: Coordinating with CodeAI');
      
      expect(result1.action).toBe('Coordinating between AIs...');
      expect(result2.action).toBe('Coordinating between AIs...');
    });
  });

  describe('parseWorkingMessage - Session Management', () => {
    it('should detect new session creation', () => {
      const result = actionTracker.parseWorkingMessage('Creating new PlannerAI session...');
      expect(result.action).toBe('Initializing AI agent...');
    });

    it('should detect AI calling phase', () => {
      const result = actionTracker.parseWorkingMessage('Calling PlannerAI with task...');
      expect(result.action).toBe('Processing with AI...');
    });
  });

  describe('parseWorkingMessage - Tool Execution', () => {
    it('should detect WebFetch execution', () => {
      const result = actionTracker.parseWorkingMessage('Executing WebFetch: https://example.com');
      expect(result.action).toBe('Searching the web...');
    });

    it('should detect file reading', () => {
      const result = actionTracker.parseWorkingMessage('Executing FILE_READ: package.json');
      expect(result.action).toBe('Reading files...');
    });

    it('should detect git status check', () => {
      const result = actionTracker.parseWorkingMessage('Executing GIT_STATUS');
      expect(result.action).toBe('Checking repository status...');
    });

    it('should detect tool scanning phase', () => {
      const result = actionTracker.parseWorkingMessage('Scanning AI response for tool commands...');
      expect(result.action).toBe('Checking for tool usage...');
    });

    it('should detect completed tool execution', () => {
      const result = actionTracker.parseWorkingMessage('Executed 3 tools for PlannerAI');
      expect(result.action).toBe('Completed tool execution...');
    });
  });

  describe('parseWorkingMessage - Response Generation', () => {
    it('should detect AI response phase', () => {
      const result = actionTracker.parseWorkingMessage('PlannerAI responded: here is the analysis');
      expect(result.action).toBe('Formulating response...');
    });

    it('should detect task completion', () => {
      const result = actionTracker.parseWorkingMessage('PlannerAI completed task - response ready');
      expect(result.action).toBe('Finalizing...');
    });
  });

  describe('parseWorkingMessage - Generic Actions', () => {
    it('should handle analyzing action with agent name', () => {
      const result = actionTracker.parseWorkingMessage('CodeAI analyzing the repository structure');
      expect(result.action).toBe('CodeAI is analyzing...');
      expect(result.agent).toBe('CodeAI');
    });

    it('should handle creating action with agent name', () => {
      const result = actionTracker.parseWorkingMessage('PlannerAI creating comprehensive strategy');
      expect(result.action).toBe('PlannerAI is creating response...');
      expect(result.agent).toBe('PlannerAI');
    });

    it('should handle coordination action', () => {
      const result = actionTracker.parseWorkingMessage('coordinating with multiple AIs for solution');
      expect(result.action).toBe('Coordinating with other AIs...');
    });

    it('should default to working status', () => {
      const result = actionTracker.parseWorkingMessage('Some unknown activity happening');
      expect(result.action).toBe('AI is working...');
    });
  });

  describe('Real-world Integration Tests', () => {
    it('should handle actual log message from Enhanced intelligent routing', () => {
      const message = '🧠 Enhanced intelligent routing: Hello, can you analyze this codebase?...';
      const result = actionTracker.parseWorkingMessage(message);
      expect(result.action).toBe('Analyzing request and selecting best AI...');
    });

    it('should handle actual log message from AI processing', () => {
      const message = '🔄 PlannerAI processing: Lead this task by analyzing the problem and creating...';
      const result = actionTracker.parseWorkingMessage(message);
      expect(result.action).toBe('PlannerAI is thinking...');
      expect(result.agent).toBe('PlannerAI');
    });

    it('should handle actual coordination messages', () => {
      const message = '🔧 COORDINATION DETECTED - PlannerAI will coordinate with CodeAI...';
      const result = actionTracker.parseWorkingMessage(message);
      expect(result.action).toBe('Multi-AI coordination required...');
    });
  });

  describe('DOM element handling', () => {
    it('should handle missing DOM elements gracefully', () => {
      mockGetElementById.mockReturnValue(null as any);
      
      const newTracker = new ActionTracker();
      
      // Should not throw errors
      expect(() => {
        newTracker.updateAction('Test action');
        newTracker.clearAction();
        newTracker.parseWorkingMessage('test message');
      }).not.toThrow();
    });
  });
});