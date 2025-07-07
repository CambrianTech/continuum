/**
 * AcademyTrainingRecommendationWidget Unit Tests
 * Comprehensive testing of Academy training configuration popup widget
 */

import { describe, test, expect, beforeEach, afterEach, jest } from '@jest/globals';
import { AcademyTrainingRecommendationWidget } from '../../AcademyTrainingRecommendationWidget.js';

// Mock continuum API
const mockContinuum = {
  execute: jest.fn(),
  academy_train: jest.fn(),
  academy_getPersonaCapabilities: jest.fn(),
  academy_estimateTrainingCost: jest.fn()
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

describe('AcademyTrainingRecommendationWidget', () => {
  let widget: AcademyTrainingRecommendationWidget;
  
  beforeEach(() => {
    jest.clearAllMocks();
    widget = new AcademyTrainingRecommendationWidget();
    
    // Mock DOM methods
    Object.defineProperty(widget, 'shadowRoot', {
      value: {
        querySelector: jest.fn(),
        querySelectorAll: jest.fn(() => []),
        addEventListener: jest.fn(),
        innerHTML: '',
        classList: {
          add: jest.fn(),
          remove: jest.fn(),
          toggle: jest.fn()
        }
      },
      writable: true
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('Widget Properties', () => {
    test('should have correct widget metadata', () => {
      expect(widget.widgetName).toBe('AcademyTrainingRecommendationWidget');
      expect(widget.widgetIcon).toBe('🎓');
      expect(widget.widgetTitle).toBe('Academy Training Setup');
    });

    test('should be configured as popup overlay widget', () => {
      expect(widget.isPopupOverlay).toBe(true);
      expect(widget.overlayBackground).toBe('rgba(59, 130, 246, 0.8)'); // Blue overlay
    });
  });

  describe('Popup Display Management', () => {
    test('should show popup overlay when triggered', () => {
      const mockClassList = { add: jest.fn(), remove: jest.fn() };
      (widget as any).shadowRoot.classList = mockClassList;

      widget.showTrainingPopup({
        participantId: 'persona_ai_1',
        participantName: 'DevMaster',
        participantType: 'persona',
        capabilities: ['typescript', 'testing']
      });

      expect(mockClassList.add).toHaveBeenCalledWith('popup-visible');
      expect((widget as any).trainingConfig.participantId).toBe('persona_ai_1');
      expect((widget as any).trainingConfig.participantName).toBe('DevMaster');
    });

    test('should hide popup overlay when closed', () => {
      const mockClassList = { add: jest.fn(), remove: jest.fn() };
      (widget as any).shadowRoot.classList = mockClassList;

      widget.hideTrainingPopup();

      expect(mockClassList.remove).toHaveBeenCalledWith('popup-visible');
    });

    test('should close popup when clicking overlay background', () => {
      const hidePopupSpy = jest.spyOn(widget, 'hideTrainingPopup');
      
      const mockOverlay = { className: 'popup-overlay' };
      const clickEvent = new Event('click');
      Object.defineProperty(clickEvent, 'target', { value: mockOverlay });

      widget.setupEventListeners();
      (widget as any).shadowRoot.addEventListener.mock.calls[0][1](clickEvent);

      expect(hidePopupSpy).toHaveBeenCalled();
    });

    test('should not close popup when clicking inside content', () => {
      const hidePopupSpy = jest.spyOn(widget, 'hideTrainingPopup');
      
      const mockContent = { className: 'popup-content' };
      const clickEvent = new Event('click');
      Object.defineProperty(clickEvent, 'target', { value: mockContent });

      widget.setupEventListeners();
      (widget as any).shadowRoot.addEventListener.mock.calls[0][1](clickEvent);

      expect(hidePopupSpy).not.toHaveBeenCalled();
    });
  });

  describe('Training Configuration', () => {
    beforeEach(() => {
      (widget as any).trainingConfig = {
        participantId: 'persona_ai_1',
        participantName: 'DevMaster',
        participantType: 'persona',
        capabilities: ['typescript', 'testing'],
        trainingType: 'collaborative',
        duration: '30m',
        skillFocus: 'general',
        additionalParticipants: []
      };
    });

    test('should display participant information', () => {
      const content = widget.renderContent();

      expect(content).toContain('DevMaster');
      expect(content).toContain('persona');
      expect(content).toContain('typescript');
      expect(content).toContain('testing');
    });

    test('should show training type selection options', () => {
      const content = widget.renderContent();

      expect(content).toContain('collaborative');
      expect(content).toContain('competitive');
      expect(content).toContain('solo');
      expect(content).toContain('value="collaborative"');
    });

    test('should display duration options', () => {
      const content = widget.renderContent();

      expect(content).toContain('15m');
      expect(content).toContain('30m');
      expect(content).toContain('1h');
      expect(content).toContain('2h');
    });

    test('should show skill focus options based on participant capabilities', () => {
      const content = widget.renderContent();

      expect(content).toContain('TypeScript Excellence');
      expect(content).toContain('Testing Methodology');
      expect(content).toContain('General Development');
    });

    test('should update configuration when form values change', () => {
      const mockElement = {
        name: 'trainingType',
        value: 'competitive',
        type: 'radio',
        checked: true
      };

      widget.handleFormChange({ target: mockElement } as any);

      expect((widget as any).trainingConfig.trainingType).toBe('competitive');
    });

    test('should validate required fields before submission', () => {
      (widget as any).trainingConfig.duration = '';

      const isValid = widget.validateTrainingConfig();

      expect(isValid).toBe(false);
    });

    test('should pass validation with complete configuration', () => {
      const isValid = widget.validateTrainingConfig();

      expect(isValid).toBe(true);
    });
  });

  describe('Cost Estimation', () => {
    test('should calculate training cost based on configuration', async () => {
      mockContinuum.academy_estimateTrainingCost.mockResolvedValue({
        success: true,
        estimatedCost: 0.12,
        currency: 'USD',
        breakdown: {
          participants: 0.05,
          duration: 0.04,
          complexity: 0.03
        }
      });

      await widget.updateCostEstimate();

      expect(mockContinuum.academy_estimateTrainingCost).toHaveBeenCalledWith({
        trainingType: 'collaborative',
        duration: '30m',
        participantCount: 1,
        skillFocus: 'general'
      });

      expect((widget as any).costEstimate).toEqual({
        total: 0.12,
        currency: 'USD',
        breakdown: expect.any(Object)
      });
    });

    test('should handle cost estimation errors gracefully', async () => {
      mockContinuum.academy_estimateTrainingCost.mockRejectedValue(new Error('Estimation failed'));

      await widget.updateCostEstimate();

      expect((widget as any).costEstimate).toEqual({
        total: 0,
        currency: 'USD',
        error: 'Unable to estimate cost'
      });
    });

    test('should display cost estimate in UI', () => {
      (widget as any).costEstimate = {
        total: 0.15,
        currency: 'USD',
        breakdown: {
          participants: 0.06,
          duration: 0.05,
          complexity: 0.04
        }
      };

      const content = widget.renderContent();

      expect(content).toContain('$0.15');
      expect(content).toContain('Participants: $0.06');
      expect(content).toContain('Duration: $0.05');
      expect(content).toContain('Complexity: $0.04');
    });
  });

  describe('Additional Participants', () => {
    test('should add additional participants to training session', () => {
      widget.addAdditionalParticipant('persona_ai_2', 'TestMaster', 'persona');

      expect((widget as any).trainingConfig.additionalParticipants).toContainEqual({
        id: 'persona_ai_2',
        name: 'TestMaster',
        type: 'persona'
      });
    });

    test('should prevent duplicate participants', () => {
      widget.addAdditionalParticipant('persona_ai_1', 'DevMaster', 'persona'); // Same as main participant

      expect((widget as any).trainingConfig.additionalParticipants).toHaveLength(0);
    });

    test('should remove additional participants', () => {
      widget.addAdditionalParticipant('persona_ai_2', 'TestMaster', 'persona');
      widget.removeAdditionalParticipant('persona_ai_2');

      expect((widget as any).trainingConfig.additionalParticipants).toHaveLength(0);
    });

    test('should display additional participants in UI', () => {
      (widget as any).trainingConfig.additionalParticipants = [
        { id: 'persona_ai_2', name: 'TestMaster', type: 'persona' },
        { id: 'human_1', name: 'Joel', type: 'human' }
      ];

      const content = widget.renderContent();

      expect(content).toContain('TestMaster');
      expect(content).toContain('Joel');
      expect(content).toContain('persona');
      expect(content).toContain('human');
    });
  });

  describe('Training Session Creation', () => {
    test('should create training session with valid configuration', async () => {
      mockContinuum.academy_train.mockResolvedValue({
        success: true,
        sessionId: 'training_session_123',
        roomId: 'academy_room_456'
      });

      const createSpy = jest.spyOn(widget, 'createTrainingSession');
      
      await widget.handleStartTraining();

      expect(createSpy).toHaveBeenCalled();
      expect(mockContinuum.academy_train).toHaveBeenCalledWith({
        participantId: 'persona_ai_1',
        participantName: 'DevMaster',
        participantType: 'persona',
        capabilities: ['typescript', 'testing'],
        trainingType: 'collaborative',
        duration: '30m',
        skillFocus: 'general',
        additionalParticipants: []
      });
    });

    test('should emit training session created event', async () => {
      mockContinuum.academy_train.mockResolvedValue({
        success: true,
        sessionId: 'training_session_123',
        roomId: 'academy_room_456'
      });

      const dispatchEventSpy = jest.spyOn(widget, 'dispatchEvent');

      await widget.handleStartTraining();

      expect(dispatchEventSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'academy:training-session-created',
          detail: expect.objectContaining({
            sessionId: 'training_session_123',
            roomId: 'academy_room_456'
          })
        })
      );
    });

    test('should handle training session creation failure', async () => {
      mockContinuum.academy_train.mockResolvedValue({
        success: false,
        error: 'Academy daemon not available'
      });

      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();

      await widget.handleStartTraining();

      expect(consoleSpy).toHaveBeenCalledWith(
        '🎓 Academy Training: Failed to create session:',
        'Academy daemon not available'
      );

      consoleSpy.mockRestore();
    });

    test('should close popup after successful training creation', async () => {
      mockContinuum.academy_train.mockResolvedValue({
        success: true,
        sessionId: 'training_session_123'
      });

      const hidePopupSpy = jest.spyOn(widget, 'hideTrainingPopup');

      await widget.handleStartTraining();

      expect(hidePopupSpy).toHaveBeenCalled();
    });
  });

  describe('Form Validation & UX', () => {
    test('should disable start button with invalid configuration', () => {
      (widget as any).trainingConfig.duration = '';

      const content = widget.renderContent();

      expect(content).toContain('disabled');
      expect(content).toContain('Complete configuration to start');
    });

    test('should enable start button with valid configuration', () => {
      const content = widget.renderContent();

      expect(content).not.toContain('disabled');
      expect(content).toContain('Start Academy Training');
    });

    test('should show loading state during training creation', async () => {
      mockContinuum.academy_train.mockImplementation(() => new Promise(resolve => {
        setTimeout(() => resolve({ success: true }), 100);
      }));

      widget.handleStartTraining();

      expect((widget as any).isCreatingSession).toBe(true);

      const content = widget.renderContent();
      expect(content).toContain('Creating training session...');
    });

    test('should update cost estimate when configuration changes', () => {
      const updateCostSpy = jest.spyOn(widget, 'updateCostEstimate');

      widget.handleFormChange({
        target: { name: 'duration', value: '1h' }
      } as any);

      expect(updateCostSpy).toHaveBeenCalled();
    });
  });

  describe('Accessibility & Keyboard Navigation', () => {
    test('should close popup on Escape key', () => {
      const hidePopupSpy = jest.spyOn(widget, 'hideTrainingPopup');

      const escapeEvent = new KeyboardEvent('keydown', { key: 'Escape' });
      widget.setupEventListeners();
      document.dispatchEvent(escapeEvent);

      expect(hidePopupSpy).toHaveBeenCalled();
    });

    test('should have proper ARIA labels', () => {
      const content = widget.renderContent();

      expect(content).toContain('aria-label');
      expect(content).toContain('role="dialog"');
      expect(content).toContain('aria-modal="true"');
    });

    test('should focus first input when popup opens', () => {
      const mockFirstInput = { focus: jest.fn() };
      (widget as any).shadowRoot.querySelector = jest.fn().mockReturnValue(mockFirstInput);

      widget.showTrainingPopup({
        participantId: 'test',
        participantName: 'Test',
        participantType: 'persona',
        capabilities: []
      });

      setTimeout(() => {
        expect(mockFirstInput.focus).toHaveBeenCalled();
      }, 100);
    });
  });
});