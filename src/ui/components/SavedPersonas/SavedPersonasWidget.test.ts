/**
 * SavedPersonasWidget Tests
 * Comprehensive test suite for the SavedPersonas TypeScript widget
 */

import { SavedPersonasWidget } from './SavedPersonasWidget.js';

// Mock fetch for CSS loading
global.fetch = jest.fn();

// Mock BaseWidget
jest.mock('../shared/BaseWidget.js', () => ({
  BaseWidget: class MockBaseWidget extends HTMLElement {
    protected widgetName = 'MockWidget';
    protected widgetIcon = '🔹';
    protected widgetTitle = 'Mock Widget';
    
    constructor() {
      super();
      this.attachShadow({ mode: 'open' });
    }
    
    async connectedCallback() {}
    
    protected getContinuumAPI() {
      return {
        on: jest.fn(),
        off: jest.fn(),
        execute: jest.fn().mockResolvedValue({ personas: [] })
      };
    }
    
    protected onContinuumEvent(type: string, handler: Function) {
      // Mock implementation
    }
    
    protected executeCommand(command: string, params: any) {
      return Promise.resolve({ personas: [] });
    }
  }
}));

describe('SavedPersonasWidget', () => {
  let widget: SavedPersonasWidget;
  let mockFetch: jest.MockedFunction<typeof fetch>;

  beforeEach(() => {
    mockFetch = fetch as jest.MockedFunction<typeof fetch>;
    mockFetch.mockClear();
    
    // Mock CSS loading
    mockFetch.mockResolvedValue({
      text: () => Promise.resolve('/* mock css */')
    } as Response);
    
    // Register custom element if not already registered
    if (!customElements.get('saved-personas-test')) {
      customElements.define('saved-personas-test', SavedPersonasWidget);
    }
    
    widget = new SavedPersonasWidget();
    document.body.appendChild(widget);
  });

  afterEach(() => {
    if (widget.parentNode) {
      widget.parentNode.removeChild(widget);
    }
  });

  describe('Initialization', () => {
    it('should create widget with proper metadata', () => {
      expect(widget.widgetName).toBe('SavedPersonas');
      expect(widget.widgetIcon).toBe('👤');
      expect(widget.widgetTitle).toBe('Saved Personas');
    });

    it('should have shadow DOM', () => {
      expect(widget.shadowRoot).toBeTruthy();
    });
  });

  describe('CSS Loading', () => {
    it('should load CSS from external file', async () => {
      const cssText = await (widget as any).loadCSS();
      
      expect(mockFetch).toHaveBeenCalledWith('/src/ui/components/SavedPersonas/SavedPersonasWidget.css');
      expect(cssText).toBe('/* mock css */');
    });

    it('should handle CSS loading errors gracefully', async () => {
      mockFetch.mockRejectedValue(new Error('CSS load failed'));
      
      const cssText = await (widget as any).loadCSS();
      
      expect(cssText).toBe('/* CSS loading failed */');
    });
  });

  describe('Persona Management', () => {
    beforeEach(() => {
      // Setup mock personas
      (widget as any).personas = [
        {
          id: 'persona-1',
          name: 'Test Persona',
          status: 'graduated',
          specialization: 'testing',
          graduationScore: 95.5,
          threshold: 85
        },
        {
          id: 'persona-2',
          name: 'Training Persona',
          status: 'training',
          specialization: 'learning',
          currentScore: 72.3,
          threshold: 80
        }
      ];
    });

    it('should render persona cards', async () => {
      await widget.render();
      
      const cards = widget.shadowRoot!.querySelectorAll('.persona-card');
      expect(cards.length).toBe(2);
    });

    it('should format persona names correctly', () => {
      const shortName = (widget as any).formatPersonaName('Test');
      const longName = (widget as any).formatPersonaName('Very Long Persona Name That Should Be Truncated');
      const finetuneTest = (widget as any).formatPersonaName('fine-tune-test-123');
      
      expect(shortName).toBe('Test');
      expect(longName).toBe('Very Long Persona Na...');
      expect(finetuneTest).toBe('Fine-Tune Test');
    });

    it('should format status correctly', () => {
      expect((widget as any).formatStatus('training')).toBe('IN ACADEMY »');
      expect((widget as any).formatStatus('failed')).toBe('FAILED ⚠️');
      expect((widget as any).formatStatus('graduated')).toBe('GRADUATED ✓');
      expect((widget as any).formatStatus('loaded')).toBe('LOADED ⚡');
      expect((widget as any).formatStatus('unknown')).toBe('UNKNOWN');
    });

    it('should handle persona selection', () => {
      const mockDispatchEvent = jest.fn();
      widget.dispatchEvent = mockDispatchEvent;
      
      (widget as any).selectPersona('persona-1');
      
      expect((widget as any).selectedPersona.id).toBe('persona-1');
      expect(mockDispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'persona-selected',
          detail: expect.objectContaining({
            persona: expect.objectContaining({ id: 'persona-1' })
          })
        })
      );
    });
  });

  describe('Academy Progress', () => {
    it('should render progress for training personas', () => {
      const persona = {
        id: 'test',
        name: 'Test',
        status: 'training' as const,
        specialization: 'test',
        currentScore: 72.5,
        threshold: 80
      };
      
      const html = (widget as any).renderAcademyProgress(persona);
      
      expect(html).toContain('Score: 72.5%');
      expect(html).toContain('Target: 80.0%');
      expect(html).toContain('progress-bar');
      expect(html).toContain('threshold-marker');
    });

    it('should render progress for graduated personas', () => {
      const persona = {
        id: 'test',
        name: 'Test',
        status: 'graduated' as const,
        specialization: 'test',
        graduationScore: 95.5,
        threshold: 85
      };
      
      const html = (widget as any).renderAcademyProgress(persona);
      
      expect(html).toContain('Score: 95.5%');
      expect(html).toContain('Target: 85.0%');
    });

    it('should handle missing progress data', () => {
      const persona = {
        id: 'test',
        name: 'Test',
        status: 'loaded' as const,
        specialization: 'test'
      };
      
      const html = (widget as any).renderAcademyProgress(persona);
      
      expect(html).toBe('');
    });
  });

  describe('Event Handling', () => {
    beforeEach(() => {
      (widget as any).personas = [
        {
          id: 'persona-1',
          name: 'Test Persona',
          status: 'graduated',
          specialization: 'testing'
        }
      ];
    });

    it('should handle persona action buttons', async () => {
      const mockExecuteCommand = jest.fn().mockResolvedValue({});
      (widget as any).executeCommand = mockExecuteCommand;
      
      await (widget as any).handlePersonaAction('deploy', 'persona-1');
      
      expect(mockExecuteCommand).toHaveBeenCalledWith('persona_deploy', { personaId: 'persona-1' });
    });

    it('should handle threshold dragging initialization', () => {
      const mockEvent = {
        target: {
          dataset: { personaId: 'persona-1' },
          closest: jest.fn().mockReturnValue({ getBoundingClientRect: () => ({ left: 0, width: 100 }) }),
          classList: { add: jest.fn() }
        },
        clientX: 50,
        preventDefault: jest.fn()
      } as any;
      
      (widget as any).handleThresholdDragStart(mockEvent);
      
      expect((widget as any).dragState).toBeTruthy();
      expect((widget as any).dragState.isDragging).toBe(true);
      expect((widget as any).dragState.personaId).toBe('persona-1');
    });
  });

  describe('Mock Data', () => {
    it('should load mock data when API fails', () => {
      (widget as any).loadMockData();
      
      expect((widget as any).personas.length).toBeGreaterThan(0);
      expect((widget as any).personas[0]).toHaveProperty('id');
      expect((widget as any).personas[0]).toHaveProperty('name');
      expect((widget as any).personas[0]).toHaveProperty('status');
      expect((widget as any).personas[0]).toHaveProperty('specialization');
    });

    it('should include different persona statuses in mock data', () => {
      (widget as any).loadMockData();
      
      const statuses = (widget as any).personas.map((p: any) => p.status);
      
      expect(statuses).toContain('graduated');
      expect(statuses).toContain('training');
      expect(statuses).toContain('failed');
    });
  });

  describe('Empty State', () => {
    it('should render empty state when no personas', () => {
      (widget as any).personas = [];
      
      const html = (widget as any).renderContent();
      
      expect(html).toContain('empty-state');
      expect(html).toContain('No personas found');
      expect(html).toContain('Create your first AI persona');
    });
  });

  describe('Integration', () => {
    it('should setup continuum event listeners', () => {
      const mockOnContinuumEvent = jest.fn();
      (widget as any).onContinuumEvent = mockOnContinuumEvent;
      
      (widget as any).setupContinuumListeners();
      
      expect(mockOnContinuumEvent).toHaveBeenCalledWith('personas_updated', expect.any(Function));
      expect(mockOnContinuumEvent).toHaveBeenCalledWith('persona_added', expect.any(Function));
    });
  });
});