/**
 * Unit Tests for StatusIndicator
 */
import { StatusIndicator, AgentStatus } from '../../src/ui/StatusIndicator.js';

// Mock DOM
const mockElement = {
  className: '',
  textContent: '',
  innerHTML: ''
};

const mockGetElementById = jest.fn(() => mockElement);

global.document = {
  getElementById: mockGetElementById
} as any;

describe('StatusIndicator', () => {
  let statusIndicator: StatusIndicator;

  beforeEach(() => {
    jest.clearAllMocks();
    mockElement.className = '';
    mockElement.textContent = '';
    statusIndicator = new StatusIndicator();
  });

  describe('updateStatus', () => {
    it('should update status with correct agent name and color', () => {
      const result = statusIndicator.updateStatus('PlannerAI', 'green');
      
      expect(result).toEqual({
        agentName: 'PlannerAI',
        color: 'green',
        message: 'You are now talking to PlannerAI'
      });
      
      expect(mockElement.className).toBe('status-dot green');
      expect(mockElement.textContent).toBe('You are now talking to PlannerAI');
    });

    it('should handle red status for connecting state', () => {
      const result = statusIndicator.updateStatus('Aria', 'red');
      
      expect(result.color).toBe('red');
      expect(mockElement.className).toBe('status-dot red');
    });

    it('should handle yellow status for warning state', () => {
      const result = statusIndicator.updateStatus('CodeAI', 'yellow');
      
      expect(result.color).toBe('yellow');
      expect(mockElement.className).toBe('status-dot yellow');
    });
  });

  describe('setConnecting', () => {
    it('should set connecting state with default agent', () => {
      statusIndicator.setConnecting();
      
      expect(mockElement.className).toBe('status-dot red');
      expect(mockElement.textContent).toBe('Aria is connecting...');
    });

    it('should set connecting state with custom agent', () => {
      statusIndicator.setConnecting('CustomAI');
      
      expect(mockElement.className).toBe('status-dot red');
      expect(mockElement.textContent).toBe('CustomAI is connecting...');
    });
  });

  describe('setConnected', () => {
    it('should set connected state', () => {
      statusIndicator.setConnected('PlannerAI');
      
      expect(mockElement.className).toBe('status-dot green');
      expect(mockElement.textContent).toBe('You are now talking to PlannerAI');
    });
  });

  describe('setError', () => {
    it('should set error state', () => {
      statusIndicator.setError();
      
      expect(mockElement.className).toBe('status-dot red');
      expect(mockElement.textContent).toBe('Connection error');
    });
  });

  describe('DOM element handling', () => {
    it('should handle missing DOM elements gracefully', () => {
      mockGetElementById.mockReturnValue(null as any);
      
      const newIndicator = new StatusIndicator();
      
      // Should not throw errors
      expect(() => {
        newIndicator.updateStatus('TestAI', 'green');
        newIndicator.setConnecting();
        newIndicator.setError();
      }).not.toThrow();
    });
  });
});