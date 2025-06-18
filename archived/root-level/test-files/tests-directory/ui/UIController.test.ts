/**
 * Unit Tests for UIController
 */
import { UIController, UIState } from '../../src/ui/UIController.js';

// Mock the dependencies
jest.mock('../../src/ui/StatusIndicator.js');
jest.mock('../../src/ui/ActionTracker.js');

import { StatusIndicator } from '../../src/ui/StatusIndicator.js';
import { ActionTracker } from '../../src/ui/ActionTracker.js';

const MockStatusIndicator = StatusIndicator as jest.MockedClass<typeof StatusIndicator>;
const MockActionTracker = ActionTracker as jest.MockedClass<typeof ActionTracker>;

describe('UIController', () => {
  let uiController: UIController;
  let mockStatusIndicator: jest.Mocked<StatusIndicator>;
  let mockActionTracker: jest.Mocked<ActionTracker>;

  beforeEach(() => {
    jest.clearAllMocks();
    
    mockStatusIndicator = {
      updateStatus: jest.fn(),
      setConnecting: jest.fn(),
      setConnected: jest.fn(),
      setError: jest.fn()
    } as any;

    mockActionTracker = {
      updateAction: jest.fn(),
      clearAction: jest.fn(),
      parseWorkingMessage: jest.fn()
    } as any;

    MockStatusIndicator.mockImplementation(() => mockStatusIndicator);
    MockActionTracker.mockImplementation(() => mockActionTracker);

    uiController = new UIController();
  });

  describe('initialization', () => {
    it('should create StatusIndicator and ActionTracker instances', () => {
      expect(MockStatusIndicator).toHaveBeenCalledTimes(1);
      expect(MockActionTracker).toHaveBeenCalledTimes(1);
    });

    it('should initialize with correct default state', () => {
      const state = uiController.getCurrentState();
      expect(state).toEqual({
        currentAgent: 'Aria',
        connectionStatus: 'connecting',
        currentAction: ''
      });
    });

    it('should call initialize methods', () => {
      uiController.initialize();
      
      expect(mockStatusIndicator.setConnecting).toHaveBeenCalledWith('Aria');
      expect(mockActionTracker.updateAction).toHaveBeenCalledWith('Initializing...');
    });
  });

  describe('connection state management', () => {
    it('should handle onConnected correctly', () => {
      uiController.onConnected();
      
      expect(mockStatusIndicator.setConnected).toHaveBeenCalledWith('Aria');
      expect(mockActionTracker.updateAction).toHaveBeenCalledWith('Ready');
      
      const state = uiController.getCurrentState();
      expect(state.connectionStatus).toBe('connected');
    });

    it('should handle onError correctly', () => {
      uiController.onError();
      
      expect(mockStatusIndicator.setError).toHaveBeenCalledTimes(1);
      expect(mockActionTracker.clearAction).toHaveBeenCalledTimes(1);
      
      const state = uiController.getCurrentState();
      expect(state.connectionStatus).toBe('error');
    });
  });

  describe('working message handling', () => {
    it('should process working message and update UI', () => {
      const mockActionStatus = {
        action: 'PlannerAI is thinking...',
        agent: 'PlannerAI',
        formatted: '<em>PlannerAI is thinking...</em>'
      };

      mockActionTracker.parseWorkingMessage.mockReturnValue(mockActionStatus);

      uiController.onWorkingMessage('PlannerAI processing: some task');

      expect(mockActionTracker.parseWorkingMessage).toHaveBeenCalledWith('PlannerAI processing: some task');
      expect(mockStatusIndicator.setConnected).toHaveBeenCalledWith('PlannerAI');
      expect(mockActionTracker.updateAction).toHaveBeenCalledWith('PlannerAI is thinking...');

      const state = uiController.getCurrentState();
      expect(state.currentAgent).toBe('PlannerAI');
      expect(state.currentAction).toBe('PlannerAI is thinking...');
    });

    it('should not update agent if generic AI detected', () => {
      const mockActionStatus = {
        action: 'AI is working...',
        agent: 'AI',
        formatted: '<em>AI is working...</em>'
      };

      mockActionTracker.parseWorkingMessage.mockReturnValue(mockActionStatus);

      uiController.onWorkingMessage('AI processing: some task');

      expect(mockActionTracker.parseWorkingMessage).toHaveBeenCalledWith('AI processing: some task');
      expect(mockStatusIndicator.setConnected).not.toHaveBeenCalled();
      expect(mockActionTracker.updateAction).toHaveBeenCalledWith('AI is working...');

      const state = uiController.getCurrentState();
      expect(state.currentAgent).toBe('Aria'); // Should remain unchanged
    });
  });

  describe('task lifecycle', () => {
    it('should handle task start', () => {
      uiController.onTaskStart('Analyze this codebase');
      
      expect(mockActionTracker.updateAction).toHaveBeenCalledWith('Processing your request...');
    });

    it('should handle task result', () => {
      uiController.onResult('CodeAI');
      
      expect(mockStatusIndicator.setConnected).toHaveBeenCalledWith('CodeAI');
      expect(mockActionTracker.clearAction).toHaveBeenCalledTimes(1);
      
      const state = uiController.getCurrentState();
      expect(state.currentAgent).toBe('CodeAI');
    });
  });

  describe('state management', () => {
    it('should return immutable copy of state', () => {
      const state1 = uiController.getCurrentState();
      const state2 = uiController.getCurrentState();
      
      expect(state1).toEqual(state2);
      expect(state1).not.toBe(state2); // Different object references
    });

    it('should track state changes through lifecycle', () => {
      // Initial state
      let state = uiController.getCurrentState();
      expect(state.connectionStatus).toBe('connecting');
      expect(state.currentAgent).toBe('Aria');

      // Connect
      uiController.onConnected();
      state = uiController.getCurrentState();
      expect(state.connectionStatus).toBe('connected');

      // Working message changes agent
      const mockActionStatus = {
        action: 'CodeAI is implementing...',
        agent: 'CodeAI',
        formatted: '<em>CodeAI is implementing...</em>'
      };
      mockActionTracker.parseWorkingMessage.mockReturnValue(mockActionStatus);
      
      uiController.onWorkingMessage('CodeAI processing: implement feature');
      state = uiController.getCurrentState();
      expect(state.currentAgent).toBe('CodeAI');
      expect(state.currentAction).toBe('CodeAI is implementing...');

      // Result updates
      uiController.onResult('GeneralAI');
      state = uiController.getCurrentState();
      expect(state.currentAgent).toBe('GeneralAI');
    });
  });

  describe('error handling', () => {
    it('should handle errors in working message processing', () => {
      mockActionTracker.parseWorkingMessage.mockImplementation(() => {
        throw new Error('Parsing error');
      });

      // Should not throw
      expect(() => {
        uiController.onWorkingMessage('invalid message');
      }).not.toThrow();
    });
  });
});