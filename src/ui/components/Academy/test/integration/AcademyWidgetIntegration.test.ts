/**
 * Academy Widget Integration Tests
 * Tests Academy widgets working together as a cohesive system
 */

import { describe, test, expect, beforeEach, afterEach, jest } from '@jest/globals';
import { AcademyStatusWidget } from '../../AcademyStatusWidget.js';
import { AcademyTrainingRecommendationWidget } from '../../AcademyTrainingRecommendationWidget.js';
import { AcademySidebarWidget } from '../../AcademySidebarWidget.js';

// Mock continuum API with realistic Academy system responses
const mockContinuum = {
  execute: jest.fn(),
  academy_status: jest.fn(),
  academy_train: jest.fn(),
  academy_spawn: jest.fn(),
  addEventListener: jest.fn(),
  removeEventListener: jest.fn()
};

// Mock DOM environment for widget testing
class MockShadowRoot {
  innerHTML = '';
  classList = {
    add: jest.fn(),
    remove: jest.fn(),
    toggle: jest.fn(),
    contains: jest.fn()
  };
  
  querySelector = jest.fn();
  querySelectorAll = jest.fn(() => []);
  addEventListener = jest.fn();
  removeEventListener = jest.fn();
  dispatchEvent = jest.fn();
}

// Mock BaseWidget for testing
jest.mock('../../shared/BaseWidget.js', () => ({
  BaseWidget: class MockBaseWidget {
    shadowRoot = new MockShadowRoot();
    widgetName = '';
    widgetIcon = '';
    widgetTitle = '';
    
    executeCommand = jest.fn();
    getContinuumAPI = jest.fn(() => mockContinuum);
    isContinuumConnected = jest.fn(() => true);
    onContinuumEvent = jest.fn();
    render = jest.fn();
    update = jest.fn();
    dispatchEvent = jest.fn();
    
    protected async initializeWidget(): Promise<void> {}
    setupEventListeners(): void {}
    renderContent(): string { return ''; }
  }
}));

describe('Academy Widget Integration', () => {
  let statusWidget: AcademyStatusWidget;
  let trainingWidget: AcademyTrainingRecommendationWidget;
  let sidebarWidget: AcademySidebarWidget;

  beforeEach(() => {
    jest.clearAllMocks();
    
    statusWidget = new AcademyStatusWidget();
    trainingWidget = new AcademyTrainingRecommendationWidget();
    sidebarWidget = new AcademySidebarWidget();

    // Mock realistic Academy system state
    mockContinuum.academy_status.mockResolvedValue({
      success: true,
      data: {
        activePersonas: 5,
        trainingSessions: [
          {
            id: 'session_1',
            participants: ['persona_ai_1', 'persona_ai_2'],
            progress: 67,
            estimatedCompletion: '3 minutes',
            type: 'collaborative'
          }
        ],
        p2pNetwork: {
          connectedNodes: 12,
          syncStatus: 'healthy',
          lastSync: '2 minutes ago',
          pendingGenomes: 3
        },
        recentGenomes: [
          {
            id: 'genome_001',
            name: 'TypeScript Optimization',
            discoveredBy: 'FormulaMaster',
            performance: 94.7,
            timestamp: '5 minutes ago'
          }
        ],
        systemHealth: 'excellent'
      }
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('Academy Status to Training Flow', () => {
    test('should trigger training popup from status widget action', async () => {
      // Setup status widget with training action
      await statusWidget.loadAcademyStatus();
      
      // Mock training popup show method
      const showPopupSpy = jest.spyOn(trainingWidget, 'showTrainingPopup');
      
      // Simulate clicking "Start Training" quick action in status widget
      await statusWidget.handleQuickAction('start-training');
      
      // Should trigger training popup with default configuration
      expect(showPopupSpy).toHaveBeenCalledWith({
        participantId: 'system',
        participantName: 'Academy System',
        participantType: 'ai-system',
        capabilities: ['general-training', 'collaborative-learning'],
        trainingType: 'collaborative',
        suggestedDuration: '30m'
      });
    });

    test('should update status widget after training session creation', async () => {
      // Mock successful training creation
      mockContinuum.academy_train.mockResolvedValue({
        success: true,
        sessionId: 'new_session_123',
        roomId: 'academy_room_456'
      });

      // Setup training widget configuration
      (trainingWidget as any).trainingConfig = {
        participantId: 'persona_ai_1',
        participantName: 'DevMaster',
        trainingType: 'collaborative',
        duration: '30m'
      };

      // Mock status widget update method
      const loadStatusSpy = jest.spyOn(statusWidget, 'loadAcademyStatus');

      // Create training session
      await trainingWidget.handleStartTraining();

      // Status widget should automatically refresh to show new session
      expect(loadStatusSpy).toHaveBeenCalled();
    });
  });

  describe('Sidebar to Academy Widget Navigation', () => {
    test('should switch to Academy mode when Academy tab clicked', () => {
      // Mock Academy tab click in sidebar
      sidebarWidget.switchToAcademyMode();

      // Should emit academy mode event
      expect((sidebarWidget as any).dispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'academy:mode-activated',
          detail: { mode: 'academy' }
        })
      );
    });

    test('should show Academy widgets when in Academy mode', () => {
      sidebarWidget.switchToAcademyMode();

      const content = sidebarWidget.renderContent();

      expect(content).toContain('academy-status-widget');
      expect(content).toContain('academy-training-recommendation-widget');
      expect(content).toContain('Academy Personas');
      expect(content).toContain('Training Sessions');
    });

    test('should hide Academy widgets when switching back to General mode', () => {
      sidebarWidget.switchToGeneralMode();

      const content = sidebarWidget.renderContent();

      expect(content).not.toContain('academy-status-widget');
      expect(content).toContain('user-selector');
      expect(content).toContain('active-projects');
    });
  });

  describe('Cross-Widget Event Communication', () => {
    test('should propagate Academy events between widgets', () => {
      // Setup event listeners
      statusWidget.setupContinuumListeners();
      trainingWidget.setupContinuumListeners();
      sidebarWidget.setupContinuumListeners();

      // Mock Academy training started event
      const trainingStartedEvent = {
        type: 'academy:training-started',
        data: {
          sessionId: 'session_123',
          participants: ['persona_ai_1', 'persona_ai_2'],
          trainingType: 'collaborative'
        }
      };

      // Emit event from training widget
      (trainingWidget as any).onContinuumEvent.mock.calls
        .find(call => call[0] === 'academy:training-started')[1](trainingStartedEvent);

      // Status widget should update its display
      expect((statusWidget as any).onContinuumEvent).toHaveBeenCalledWith(
        'academy:training-started',
        expect.any(Function)
      );

      // Sidebar widget should update Academy personas
      expect((sidebarWidget as any).onContinuumEvent).toHaveBeenCalledWith(
        'academy:training-started', 
        expect.any(Function)
      );
    });

    test('should handle Academy genome discovery events', () => {
      const genomeDiscoveredEvent = {
        type: 'academy:genome-discovered',
        data: {
          genomeId: 'genome_new_001',
          name: 'Advanced Error Handling',
          discoveredBy: 'SynthesisEngine',
          performance: 96.3,
          capabilities: ['error-recovery', 'graceful-degradation']
        }
      };

      // Emit genome discovery event
      (statusWidget as any).onContinuumEvent.mock.calls
        .find(call => call[0] === 'academy:genome-discovered')[1](genomeDiscoveredEvent);

      // Status widget should add new genome to recent discoveries
      expect((statusWidget as any).academyStatus.recentGenomes).toContainEqual(
        expect.objectContaining({
          name: 'Advanced Error Handling',
          performance: 96.3
        })
      );
    });
  });

  describe('Academy Command Integration', () => {
    test('should execute Academy commands through continuum API', async () => {
      // Test status command execution
      await statusWidget.loadAcademyStatus();
      expect(mockContinuum.academy_status).toHaveBeenCalledWith({
        includeP2P: true,
        includeGenomes: true,
        includeTraining: true
      });

      // Test training command execution
      mockContinuum.academy_train.mockResolvedValue({ success: true });
      
      (trainingWidget as any).trainingConfig = {
        participantId: 'persona_ai_1',
        trainingType: 'collaborative',
        duration: '30m'
      };

      await trainingWidget.handleStartTraining();
      expect(mockContinuum.academy_train).toHaveBeenCalledWith(
        expect.objectContaining({
          participantId: 'persona_ai_1',
          trainingType: 'collaborative',
          duration: '30m'
        })
      );

      // Test spawn persona command
      await statusWidget.handleQuickAction('spawn-persona');
      expect(mockContinuum.academy_spawn).toHaveBeenCalledWith({
        personaType: 'development',
        capabilities: ['typescript', 'testing', 'architecture']
      });
    });

    test('should handle Academy command failures gracefully', async () => {
      // Mock command failure
      mockContinuum.academy_status.mockRejectedValue(new Error('Academy daemon offline'));

      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();

      await statusWidget.loadAcademyStatus();

      // Should log error and set offline status
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Failed to load Academy status'),
        expect.any(Error)
      );

      expect((statusWidget as any).academyStatus.systemHealth).toBe('offline');

      consoleSpy.mockRestore();
    });
  });

  describe('Academy Data Flow & State Management', () => {
    test('should maintain consistent state across widgets', async () => {
      // Load initial Academy state
      await statusWidget.loadAcademyStatus();
      
      const academyState = (statusWidget as any).academyStatus;

      // Verify state contains expected Academy data
      expect(academyState).toEqual(expect.objectContaining({
        activePersonas: 5,
        trainingSessions: expect.arrayContaining([
          expect.objectContaining({
            id: 'session_1',
            progress: 67,
            type: 'collaborative'
          })
        ]),
        p2pNetwork: expect.objectContaining({
          connectedNodes: 12,
          syncStatus: 'healthy'
        }),
        recentGenomes: expect.arrayContaining([
          expect.objectContaining({
            name: 'TypeScript Optimization',
            performance: 94.7
          })
        ])
      }));
    });

    test('should update all widgets when Academy state changes', () => {
      // Mock state change event
      const stateChangeEvent = {
        type: 'academy:status-updated',
        data: {
          activePersonas: 7, // Increased from 5
          newTrainingSession: {
            id: 'session_2',
            participants: ['persona_ai_3'],
            progress: 12,
            type: 'solo'
          }
        }
      };

      // All widgets should handle the state update
      [statusWidget, trainingWidget, sidebarWidget].forEach(widget => {
        const updateSpy = jest.spyOn(widget, 'update');
        
        // Simulate receiving the event
        (widget as any).onContinuumEvent.mock.calls
          .find(call => call[0] === 'academy:status-updated')?.[1](stateChangeEvent);

        expect(updateSpy).toHaveBeenCalled();
      });
    });
  });

  describe('Performance & Resource Management', () => {
    test('should efficiently manage status update intervals', () => {
      jest.useFakeTimers();

      // Start status updates
      statusWidget.startStatusUpdates();

      // Should not overload with rapid updates
      const loadStatusSpy = jest.spyOn(statusWidget, 'loadAcademyStatus').mockResolvedValue();

      jest.advanceTimersByTime(5000); // 5 seconds
      expect(loadStatusSpy).toHaveBeenCalledTimes(1);

      jest.advanceTimersByTime(5000); // 10 seconds total
      expect(loadStatusSpy).toHaveBeenCalledTimes(2);

      // Clean up
      statusWidget.stopStatusUpdates();
      jest.useRealTimers();
    });

    test('should handle rapid widget creation and destruction', () => {
      // Create multiple widget instances
      const widgets = Array.from({ length: 10 }, () => new AcademyStatusWidget());

      // All should initialize without conflicts
      widgets.forEach((widget, index) => {
        expect(widget.widgetName).toBe('AcademyStatusWidget');
        expect((widget as any).executeCommand).toBeDefined();
      });

      // Cleanup should not cause errors
      widgets.forEach(widget => {
        widget.stopStatusUpdates();
      });
    });
  });

  describe('Error Recovery & Resilience', () => {
    test('should recover from Academy daemon disconnection', async () => {
      // Initially connected
      await statusWidget.loadAcademyStatus();
      expect((statusWidget as any).academyStatus.systemHealth).toBe('excellent');

      // Simulate disconnection
      mockContinuum.academy_status.mockRejectedValue(new Error('Connection lost'));
      
      await statusWidget.loadAcademyStatus();
      expect((statusWidget as any).academyStatus.systemHealth).toBe('offline');

      // Simulate reconnection
      mockContinuum.academy_status.mockResolvedValue({
        success: true,
        data: { systemHealth: 'excellent', activePersonas: 5 }
      });

      await statusWidget.loadAcademyStatus();
      expect((statusWidget as any).academyStatus.systemHealth).toBe('excellent');
    });

    test('should handle malformed Academy data gracefully', async () => {
      // Mock malformed response
      mockContinuum.academy_status.mockResolvedValue({
        success: true,
        data: {
          activePersonas: 'invalid',
          trainingSessions: null,
          p2pNetwork: undefined,
          recentGenomes: 'not-an-array'
        }
      });

      await statusWidget.loadAcademyStatus();

      // Should normalize malformed data
      expect((statusWidget as any).academyStatus).toEqual(expect.objectContaining({
        activePersonas: 0,
        trainingSessions: [],
        p2pNetwork: expect.objectContaining({
          connectedNodes: 0,
          syncStatus: 'unknown'
        }),
        recentGenomes: []
      }));
    });
  });
});