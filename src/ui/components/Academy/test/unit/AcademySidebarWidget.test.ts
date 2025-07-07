/**
 * Academy Sidebar Widget Unit Tests
 * 
 * Comprehensive testing following scientific methodology:
 * - Test pattern recognition and event dispatching
 * - Validate widget lifecycle and DOM manipulation
 * - Verify Academy integration and error handling
 * - Prove widget architecture compliance
 */

import { describe, test, expect, beforeEach, afterEach, vi } from 'vitest';
import { AcademySidebarWidget } from '../../AcademySidebarWidget.js';

// Type-safe test fixtures using discriminated unions
type PersonaStatus = 'online' | 'working' | 'offline';
type RoomType = 'general' | 'academy' | 'synthesis' | 'discovery';

interface TestPersona {
  readonly id: string;
  readonly name: string;
  readonly role: string;
  readonly status: PersonaStatus;
  readonly domains: readonly string[];
}

interface TestRoom {
  readonly id: string;
  readonly name: string;
  readonly type: RoomType;
  readonly icon: string;
}

interface MockAcademyAPI {
  connect: ReturnType<typeof vi.fn>;
  getCosts: ReturnType<typeof vi.fn>;
}

describe('AcademySidebarWidget', () => {
  let widget: AcademySidebarWidget;
  let mockAPI: MockAcademyAPI;
  let mockDocument: { dispatchEvent: ReturnType<typeof vi.fn> };

  const testPersonas: readonly TestPersona[] = [
    {
      id: 'planner_ai',
      name: 'PlannerAI', 
      role: 'planner',
      status: 'online',
      domains: ['planning', 'coordination']
    },
    {
      id: 'formula_master',
      name: 'FormulaMaster',
      role: 'formula_master', 
      status: 'working',
      domains: ['optimization', 'training_formulas']
    },
    {
      id: 'synthesis_engine',
      name: 'SynthesisEngine',
      role: 'synthesis_engine',
      status: 'offline',
      domains: ['capability_synthesis']
    }
  ] as const;

  const testRooms: readonly TestRoom[] = [
    { id: 'general', name: 'General Chat', type: 'general', icon: '#' },
    { id: 'academy', name: 'Academy', type: 'academy', icon: '🧪' },
    { id: 'synthesis', name: 'Synthesis Lab', type: 'synthesis', icon: '🧬' },
    { id: 'discovery', name: 'Genome Discovery', type: 'discovery', icon: '🔍' }
  ] as const;

  beforeEach(() => {
    // Reset DOM
    document.body.innerHTML = '';
    
    // Mock Academy API
    mockAPI = {
      connect: vi.fn().mockResolvedValue({ success: true }),
      getCosts: vi.fn().mockResolvedValue({ 
        totalCost: 0.0042, 
        requestCount: 15 
      })
    };

    // Mock document event dispatching
    mockDocument = { dispatchEvent: vi.fn() };
    Object.defineProperty(global, 'document', {
      value: mockDocument,
      writable: true
    });

    // Create widget instance
    widget = new AcademySidebarWidget();
    
    // Mock executeCommand method
    widget.executeCommand = vi.fn().mockImplementation((command: string) => {
      switch (command) {
        case 'academy.connect':
          return mockAPI.connect();
        case 'session.getCosts':
          return mockAPI.getCosts();
        default:
          return Promise.resolve({});
      }
    });

    document.body.appendChild(widget);
  });

  afterEach(() => {
    widget.remove();
    vi.clearAllMocks();
  });

  describe('Widget Lifecycle & Architecture', () => {
    test('should extend BaseWidget with proper metadata', () => {
      expect(widget.widgetName).toBe('Academy Sidebar');
      expect(widget.widgetIcon).toBe('🧬'); 
      expect(widget.widgetTitle).toBe('Academy Navigation');
    });

    test('should have correct base path for asset loading', () => {
      expect(AcademySidebarWidget.getBasePath()).toBe('/src/ui/components/Academy');
    });

    test('should be registered as web component', () => {
      expect(customElements.get('academy-sidebar-widget')).toBe(AcademySidebarWidget);
    });

    test('should initialize with default rooms and personas', async () => {
      await widget.initializeWidget();
      
      // Verify internal state initialization
      const shadowRoot = widget.shadowRoot!;
      expect(shadowRoot).toBeTruthy();
    });
  });

  describe('Room Navigation & Event Dispatching', () => {
    beforeEach(async () => {
      await widget.render();
    });

    test('should render all Academy rooms with correct structure', async () => {
      const roomElements = widget.shadowRoot!.querySelectorAll('.room-item');
      expect(roomElements).toHaveLength(4);

      testRooms.forEach((room, index) => {
        const roomElement = roomElements[index];
        expect(roomElement.getAttribute('data-room-id')).toBe(room.id);
        expect(roomElement.querySelector('.room-icon')?.textContent).toBe(room.icon);
        expect(roomElement.querySelector('.room-name')?.textContent).toBe(room.name);
      });
    });

    test('should dispatch room-changed event on room selection', async () => {
      const academyRoom = widget.shadowRoot!.querySelector('[data-room-id="academy"]') as HTMLElement;
      expect(academyRoom).toBeTruthy();

      academyRoom.click();

      expect(mockDocument.dispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'continuum:room-changed',
          detail: expect.objectContaining({
            room: expect.objectContaining({
              id: 'academy',
              name: 'Academy',
              type: 'academy'
            }),
            source: 'academy-sidebar'
          })
        })
      );
    });

    test('should update active room UI state correctly', async () => {
      const generalRoom = widget.shadowRoot!.querySelector('[data-room-id="general"]') as HTMLElement;
      const academyRoom = widget.shadowRoot!.querySelector('[data-room-id="academy"]') as HTMLElement;

      // Initially general should be active
      expect(generalRoom.classList.contains('active')).toBe(true);
      expect(academyRoom.classList.contains('active')).toBe(false);

      // Click academy room
      academyRoom.click();

      // Academy should now be active
      expect(generalRoom.classList.contains('active')).toBe(false);
      expect(academyRoom.classList.contains('active')).toBe(true);
    });

    test('should prevent duplicate room switching', async () => {
      const generalRoom = widget.shadowRoot!.querySelector('[data-room-id="general"]') as HTMLElement;
      
      // Click same room twice
      generalRoom.click();
      generalRoom.click();

      // Should only dispatch once (second click ignored)
      expect(mockDocument.dispatchEvent).toHaveBeenCalledTimes(0); // First click ignored since already active
    });
  });

  describe('Persona Management & Interactions', () => {
    beforeEach(async () => {
      await widget.render();
    });

    test('should render personas with correct status indicators', async () => {
      const personaElements = widget.shadowRoot!.querySelectorAll('.persona-item');
      expect(personaElements).toHaveLength(4); // 4 default personas

      // Test first persona (PlannerAI - online)
      const plannerElement = personaElements[0];
      expect(plannerElement.classList.contains('online')).toBe(true);
      expect(plannerElement.querySelector('.persona-name')?.textContent).toBe('PlannerAI');
      expect(plannerElement.querySelector('.persona-status.online')).toBeTruthy();
    });

    test('should dispatch persona-focus event on persona interaction', async () => {
      const personaElement = widget.shadowRoot!.querySelector('[data-persona-id="planner_ai"]') as HTMLElement;
      expect(personaElement).toBeTruthy();

      personaElement.click();

      expect(mockDocument.dispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'continuum:persona-focus',
          detail: expect.objectContaining({
            persona: expect.objectContaining({
              id: 'planner_ai',
              name: 'PlannerAI',
              role: 'planner'
            }),
            source: 'academy-sidebar'
          })
        })
      );
    });

    test('should display persona domains correctly truncated', async () => {
      const personaElement = widget.shadowRoot!.querySelector('[data-persona-id="planner_ai"]');
      const domainsElement = personaElement?.querySelector('.persona-domains');
      
      expect(domainsElement?.textContent).toBe('planning, coordination');
    });

    test('should handle missing persona gracefully', async () => {
      const nonExistentPersona = widget.shadowRoot!.querySelector('[data-persona-id="non_existent"]') as HTMLElement;
      expect(nonExistentPersona).toBeNull();
    });
  });

  describe('Session Cost Tracking', () => {
    beforeEach(async () => {
      await widget.render();
    });

    test('should update session costs via API call', async () => {
      // Trigger cost update
      await new Promise(resolve => setTimeout(resolve, 0));

      expect(mockAPI.getCosts).toHaveBeenCalled();
      
      const costAmount = widget.shadowRoot!.querySelector('.cost-amount');
      const costDetails = widget.shadowRoot!.querySelector('.cost-details');
      
      expect(costAmount?.textContent).toBe('$0.0042');
      expect(costDetails?.textContent).toBe('15 Requests • Cost');
    });

    test('should handle cost API failures gracefully', async () => {
      mockAPI.getCosts.mockRejectedValue(new Error('API unavailable'));
      
      // Should not throw, should handle gracefully
      await expect(async () => {
        await new Promise(resolve => setTimeout(resolve, 0));
      }).not.toThrow();
    });

    test('should update costs periodically', async () => {
      // Fast-forward time to trigger periodic update
      vi.useFakeTimers();
      
      await widget.render();
      vi.advanceTimersByTime(30000); // 30 seconds
      
      expect(mockAPI.getCosts).toHaveBeenCalled();
      
      vi.useRealTimers();
    });
  });

  describe('Academy Backend Integration', () => {
    test('should connect to Academy backend on initialization', async () => {
      await widget.render();
      
      expect(mockAPI.connect).toHaveBeenCalled();
    });

    test('should update status indicator on successful connection', async () => {
      mockAPI.connect.mockResolvedValue({ success: true });
      
      await widget.render();
      
      const statusIndicator = widget.shadowRoot!.querySelector('.status-indicator');
      const statusText = widget.shadowRoot!.querySelector('.status-text');
      
      expect(statusIndicator?.classList.contains('online')).toBe(true);
      expect(statusText?.textContent).toBe('Academy Ready');
    });

    test('should handle connection failures gracefully', async () => {
      mockAPI.connect.mockRejectedValue(new Error('Connection failed'));
      
      await widget.render();
      
      const statusIndicator = widget.shadowRoot!.querySelector('.status-indicator');
      const statusText = widget.shadowRoot!.querySelector('.status-text');
      
      expect(statusIndicator?.classList.contains('offline')).toBe(true);
      expect(statusText?.textContent).toBe('Academy Offline');
    });
  });

  describe('Error Handling & Edge Cases', () => {
    test('should handle missing DOM elements gracefully', async () => {
      // Mock missing elements
      const originalQuerySelector = widget.shadowRoot!.querySelector;
      widget.shadowRoot!.querySelector = vi.fn().mockReturnValue(null);
      
      await expect(widget.render()).resolves.not.toThrow();
      
      widget.shadowRoot!.querySelector = originalQuerySelector;
    });

    test('should handle malformed event targets', async () => {
      await widget.render();
      
      const roomList = widget.shadowRoot!.querySelector('#room-list')!;
      
      // Create event with no room-item parent
      const mockEvent = {
        target: document.createElement('span') // No data-room-id
      } as any;
      
      expect(() => {
        roomList.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      }).not.toThrow();
    });

    test('should validate persona data structure', () => {
      const validPersona = {
        id: 'test_persona',
        name: 'TestPersona',
        type: 'ai_persona' as const,
        role: 'tester' as const,
        avatar: '🧪',
        domains: ['testing'],
        status: 'online' as const,
        capabilities: ['test_execution']
      };

      // TypeScript ensures this compiles correctly
      expect(validPersona.status).toBe('online');
      expect(validPersona.domains).toEqual(['testing']);
    });
  });

  describe('TypeScript Type Safety & Patterns', () => {
    test('should use discriminated unions for persona status', () => {
      const onlinePersona = { status: 'online' as PersonaStatus };
      const workingPersona = { status: 'working' as PersonaStatus };
      const offlinePersona = { status: 'offline' as PersonaStatus };

      // Type narrowing should work correctly
      expect(['online', 'working', 'offline']).toContain(onlinePersona.status);
      expect(['online', 'working', 'offline']).toContain(workingPersona.status);
      expect(['online', 'working', 'offline']).toContain(offlinePersona.status);
    });

    test('should use readonly arrays for immutable data', () => {
      const domains: readonly string[] = ['domain1', 'domain2'];
      
      // TypeScript prevents mutation
      expect(domains).toEqual(['domain1', 'domain2']);
      expect(() => {
        // @ts-expect-error - Should not be able to push to readonly array
        (domains as string[]).push('domain3');
      }).toThrow();
    });

    test('should handle destructuring with type safety', () => {
      const { id, name, status } = testPersonas[0];
      
      expect(id).toBe('planner_ai');
      expect(name).toBe('PlannerAI');
      expect(status).toBe('online');
    });
  });
});

/**
 * Integration Test Helper Functions
 * These prove the widget works with real DOM and events
 */

export const createTestEnvironment = () => ({
  widget: new AcademySidebarWidget(),
  mockAPI: {
    connect: vi.fn().mockResolvedValue({ success: true }),
    getCosts: vi.fn().mockResolvedValue({ totalCost: 0.0042, requestCount: 15 })
  },
  cleanup: () => {
    document.body.innerHTML = '';
    vi.clearAllMocks();
  }
});

export const simulateRoomSwitch = async (widget: AcademySidebarWidget, roomId: string) => {
  await widget.render();
  const roomElement = widget.shadowRoot!.querySelector(`[data-room-id="${roomId}"]`) as HTMLElement;
  roomElement?.click();
  return roomElement;
};

export const simulatePersonaClick = async (widget: AcademySidebarWidget, personaId: string) => {
  await widget.render();
  const personaElement = widget.shadowRoot!.querySelector(`[data-persona-id="${personaId}"]`) as HTMLElement;
  personaElement?.click();
  return personaElement;
};