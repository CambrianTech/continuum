/**
 * AcademyStatusWidget Unit Tests
 * Comprehensive testing of Academy system monitoring widget
 */

import { describe, test, expect, beforeEach, afterEach, jest } from '@jest/globals';
import { AcademyStatusWidget } from '../../AcademyStatusWidget.js';

// Mock continuum API
const mockContinuum = {
  execute: jest.fn(),
  addEventListener: jest.fn(),
  removeEventListener: jest.fn()
};

// Mock BaseWidget methods
jest.mock('../../shared/BaseWidget.js', () => ({
  BaseWidget: class MockBaseWidget {
    shadowRoot = document.createElement('div');
    widgetName = '';
    widgetIcon = '';
    widgetTitle = '';
    
    executeCommand = jest.fn();
    getContinuumAPI = jest.fn(() => mockContinuum);
    isContinuumConnected = jest.fn(() => true);
    onContinuumEvent = jest.fn();
    render = jest.fn();
    update = jest.fn();
    
    protected async initializeWidget(): Promise<void> {}
    setupEventListeners(): void {}
    renderContent(): string { return ''; }
  }
}));

describe('AcademyStatusWidget', () => {
  let widget: AcademyStatusWidget;
  
  beforeEach(() => {
    jest.clearAllMocks();
    widget = new AcademyStatusWidget();
    
    // Mock DOM methods
    Object.defineProperty(widget, 'shadowRoot', {
      value: {
        querySelector: jest.fn(),
        querySelectorAll: jest.fn(() => []),
        addEventListener: jest.fn(),
        innerHTML: ''
      },
      writable: true
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('Widget Properties', () => {
    test('should have correct widget metadata', () => {
      expect(widget.widgetName).toBe('AcademyStatusWidget');
      expect(widget.widgetIcon).toBe('🎓');
      expect(widget.widgetTitle).toBe('Academy Status');
    });

    test('should declare correct CSS and HTML paths', () => {
      expect(AcademyStatusWidget.getBasePath()).toBe('/src/ui/components/Academy');
      expect(AcademyStatusWidget.getOwnCSS()).toEqual(['AcademyStatusWidget.css']);
    });
  });

  describe('Academy Status Loading', () => {
    test('should load academy status on initialization', async () => {
      const mockStatus = {
        activePersonas: 5,
        trainingSessions: 3,
        p2pNodes: 12,
        genomeLibrarySize: 847,
        systemHealth: 'excellent'
      };

      (widget as any).executeCommand = jest.fn().mockResolvedValue({
        success: true,
        data: mockStatus
      });

      await widget.loadAcademyStatus();

      expect((widget as any).executeCommand).toHaveBeenCalledWith('academy_status', {
        includeP2P: true,
        includeGenomes: true,
        includeTraining: true
      });
      expect((widget as any).academyStatus).toEqual(mockStatus);
    });

    test('should handle academy status loading failure gracefully', async () => {
      (widget as any).executeCommand = jest.fn().mockRejectedValue(new Error('Connection failed'));

      await widget.loadAcademyStatus();

      expect((widget as any).academyStatus).toEqual({
        activePersonas: 0,
        trainingSessions: 0,
        p2pNodes: 0,
        genomeLibrarySize: 0,
        systemHealth: 'offline'
      });
    });

    test('should auto-refresh status every 5 seconds', () => {
      jest.useFakeTimers();
      
      const loadStatusSpy = jest.spyOn(widget, 'loadAcademyStatus').mockResolvedValue();
      widget.startStatusUpdates();

      jest.advanceTimersByTime(5000);
      expect(loadStatusSpy).toHaveBeenCalledTimes(1);

      jest.advanceTimersByTime(5000);
      expect(loadStatusSpy).toHaveBeenCalledTimes(2);

      jest.useRealTimers();
    });
  });

  describe('Training Progress Tracking', () => {
    test('should display training sessions with progress', () => {
      (widget as any).academyStatus = {
        trainingSessions: [
          {
            id: 'session_1',
            participants: ['persona_ai_1', 'persona_ai_2'],
            progress: 67,
            estimatedCompletion: '2 minutes',
            type: 'collaborative'
          },
          {
            id: 'session_2', 
            participants: ['persona_ai_3'],
            progress: 23,
            estimatedCompletion: '8 minutes',
            type: 'solo'
          }
        ]
      };

      const content = widget.renderContent();

      expect(content).toContain('Training Sessions: 2');
      expect(content).toContain('67%');
      expect(content).toContain('23%');
      expect(content).toContain('2 minutes');
      expect(content).toContain('8 minutes');
      expect(content).toContain('collaborative');
      expect(content).toContain('solo');
    });

    test('should show no training sessions when none active', () => {
      (widget as any).academyStatus = { trainingSessions: [] };

      const content = widget.renderContent();

      expect(content).toContain('Training Sessions: 0');
      expect(content).toContain('No active training sessions');
    });
  });

  describe('P2P Network Status', () => {
    test('should display P2P network health', () => {
      (widget as any).academyStatus = {
        p2pNetwork: {
          connectedNodes: 12,
          syncStatus: 'healthy',
          lastSync: '2 minutes ago',
          pendingGenomes: 3
        }
      };

      const content = widget.renderContent();

      expect(content).toContain('P2P Network: 12 nodes');
      expect(content).toContain('healthy');
      expect(content).toContain('2 minutes ago');
      expect(content).toContain('3 pending');
    });

    test('should handle P2P network offline state', () => {
      (widget as any).academyStatus = {
        p2pNetwork: {
          connectedNodes: 0,
          syncStatus: 'offline',
          lastSync: 'never',
          pendingGenomes: 0
        }
      };

      const content = widget.renderContent();

      expect(content).toContain('P2P Network: offline');
      expect(content).toContain('⚠️');
    });
  });

  describe('Genome Discovery Feed', () => {
    test('should display recent genome discoveries', () => {
      (widget as any).academyStatus = {
        recentGenomes: [
          {
            id: 'genome_001',
            name: 'TypeScript Optimization',
            discoveredBy: 'FormulaMaster',
            performance: 94.7,
            timestamp: '5 minutes ago'
          },
          {
            id: 'genome_002', 
            name: 'Error Handling Patterns',
            discoveredBy: 'SynthesisEngine',
            performance: 89.2,
            timestamp: '12 minutes ago'
          }
        ]
      };

      const content = widget.renderContent();

      expect(content).toContain('TypeScript Optimization');
      expect(content).toContain('Error Handling Patterns');
      expect(content).toContain('94.7%');
      expect(content).toContain('89.2%');
      expect(content).toContain('FormulaMaster');
      expect(content).toContain('SynthesisEngine');
    });

    test('should show empty state when no genomes discovered', () => {
      (widget as any).academyStatus = { recentGenomes: [] };

      const content = widget.renderContent();

      expect(content).toContain('No recent genome discoveries');
    });
  });

  describe('Quick Action Controls', () => {
    test('should handle spawn persona action', async () => {
      const mockExecuteCommand = jest.fn().mockResolvedValue({ success: true });
      (widget as any).executeCommand = mockExecuteCommand;

      await widget.handleQuickAction('spawn-persona');

      expect(mockExecuteCommand).toHaveBeenCalledWith('academy_spawn', {
        personaType: 'development',
        capabilities: ['typescript', 'testing', 'architecture']
      });
    });

    test('should handle start training action', async () => {
      const mockExecuteCommand = jest.fn().mockResolvedValue({ success: true });
      (widget as any).executeCommand = mockExecuteCommand;

      await widget.handleQuickAction('start-training');

      expect(mockExecuteCommand).toHaveBeenCalledWith('academy_train', {
        trainingType: 'collaborative',
        duration: '30m',
        focus: 'general'
      });
    });

    test('should handle sync P2P action', async () => {
      const mockExecuteCommand = jest.fn().mockResolvedValue({ success: true });
      (widget as any).executeCommand = mockExecuteCommand;

      await widget.handleQuickAction('sync-p2p');

      expect(mockExecuteCommand).toHaveBeenCalledWith('academy_sync', {
        force: true,
        includeGenomes: true
      });
    });

    test('should handle discover genomes action', async () => {
      const mockExecuteCommand = jest.fn().mockResolvedValue({ success: true });
      (widget as any).executeCommand = mockExecuteCommand;

      await widget.handleQuickAction('discover-genomes');

      expect(mockExecuteCommand).toHaveBeenCalledWith('academy_discover', {
        searchType: 'capability_gaps',
        priority: 'high'
      });
    });

    test('should handle unknown action gracefully', async () => {
      const consoleSpy = jest.spyOn(console, 'warn').mockImplementation();

      await widget.handleQuickAction('unknown-action');

      expect(consoleSpy).toHaveBeenCalledWith('Unknown Academy action: unknown-action');
      consoleSpy.mockRestore();
    });
  });

  describe('Event Handling', () => {
    test('should set up continuum event listeners', () => {
      const mockOnContinuumEvent = jest.fn();
      (widget as any).onContinuumEvent = mockOnContinuumEvent;

      widget.setupContinuumListeners();

      expect(mockOnContinuumEvent).toHaveBeenCalledWith('academy:status-updated', expect.any(Function));
      expect(mockOnContinuumEvent).toHaveBeenCalledWith('academy:training-started', expect.any(Function));
      expect(mockOnContinuumEvent).toHaveBeenCalledWith('academy:training-completed', expect.any(Function));
      expect(mockOnContinuumEvent).toHaveBeenCalledWith('academy:genome-discovered', expect.any(Function));
    });

    test('should emit details events when sections are clicked', () => {
      const mockElement = {
        dataset: { action: 'training-details' },
        closest: jest.fn().mockReturnValue({ dataset: { action: 'training-details' } })
      };

      (widget as any).shadowRoot.querySelector = jest.fn().mockReturnValue(mockElement);

      const dispatchEventSpy = jest.spyOn(widget, 'dispatchEvent');

      widget.setupEventListeners();
      
      // Simulate click event
      const clickEvent = new Event('click');
      Object.defineProperty(clickEvent, 'target', { value: mockElement });
      (widget as any).shadowRoot.addEventListener.mock.calls[0][1](clickEvent);

      expect(dispatchEventSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'academy:training-details',
          detail: expect.any(Object)
        })
      );
    });
  });

  describe('Error Handling & Edge Cases', () => {
    test('should handle missing academy status gracefully', () => {
      (widget as any).academyStatus = null;

      const content = widget.renderContent();

      expect(content).toContain('Academy Status: Loading...');
      expect(content).not.toContain('undefined');
      expect(content).not.toContain('null');
    });

    test('should handle malformed academy status data', () => {
      (widget as any).academyStatus = { 
        activePersonas: 'invalid',
        trainingSessions: null,
        p2pNodes: undefined 
      };

      const content = widget.renderContent();

      expect(content).toContain('Active Personas: 0');
      expect(content).toContain('Training Sessions: 0');
      expect(content).toContain('P2P Network: 0 nodes');
    });

    test('should clean up status update timer on destroy', () => {
      widget.startStatusUpdates();
      expect((widget as any).statusUpdateInterval).toBeDefined();

      widget.stopStatusUpdates();
      expect((widget as any).statusUpdateInterval).toBeNull();
    });
  });

  describe('Performance & Resource Management', () => {
    test('should debounce rapid status updates', () => {
      jest.useFakeTimers();
      
      const loadStatusSpy = jest.spyOn(widget, 'loadAcademyStatus').mockResolvedValue();
      
      // Trigger multiple rapid updates
      widget.handleStatusUpdate({ type: 'training-started' });
      widget.handleStatusUpdate({ type: 'training-started' });
      widget.handleStatusUpdate({ type: 'training-started' });

      // Should only call once after debounce period
      jest.advanceTimersByTime(1000);
      expect(loadStatusSpy).toHaveBeenCalledTimes(1);

      jest.useRealTimers();
    });

    test('should limit genome discovery feed to reasonable size', () => {
      const manyGenomes = Array.from({ length: 100 }, (_, i) => ({
        id: `genome_${i}`,
        name: `Genome ${i}`,
        discoveredBy: 'AI',
        performance: 90,
        timestamp: '1 minute ago'
      }));

      (widget as any).academyStatus = { recentGenomes: manyGenomes };

      const content = widget.renderContent();
      
      // Should only show first 5 genomes
      expect(content.match(/genome_/g)?.length).toBeLessThanOrEqual(5);
    });
  });
});