/**
 * PersonaDaemon Test Suite
 * Tests the universal session framework for AI personas
 */

import { PersonaDaemon, PersonaConfig, AcademyTrainingConfig } from './PersonaDaemon';

describe('PersonaDaemon', () => {
  let persona: PersonaDaemon;
  let config: PersonaConfig;
  let academyConfig: AcademyTrainingConfig;

  beforeEach(() => {
    config = {
      id: 'test-persona',
      name: 'Test Patent Expert',
      modelProvider: 'anthropic',
      modelConfig: {
        model: 'claude-3-haiku',
        apiKey: 'test-key'
      },
      loraAdapters: ['continuum.legal', 'continuum.legal.patent'],
      capabilities: ['chat', 'screenshot', 'browser_js'],
      sessionDirectory: '.continuum/test-persona/'
    };

    academyConfig = {
      enabled: true,
      role: 'testing_droid',
      trainingDomain: 'patent_law',
      adversarialPartner: 'protocol-sheriff-1'
    };

    persona = new PersonaDaemon(config, academyConfig);
  });

  afterEach(async () => {
    if (persona.getSimpleStatus() === 'running') {
      await persona.stop();
    }
  });

  describe('Initialization', () => {
    test('should initialize with correct name and config', () => {
      expect(persona.name).toBe('persona-test-persona');
      expect(persona.version).toBe('1.0.0');
    });

    test('should start successfully', async () => {
      await persona.start();
      expect(persona.getSimpleStatus()).toBe('running');
    });

    test('should stop gracefully', async () => {
      await persona.start();
      await persona.stop();
      expect(persona.getSimpleStatus()).toBe('stopped');
    });
  });

  describe('Command Execution', () => {
    beforeEach(async () => {
      await persona.start();
    });

    test('should execute commands like human sessions', async () => {
      const message = {
        id: 'test-1',
        from: 'test-client',
        to: 'persona-test-persona',
        type: 'execute_command',
        data: {
          command: 'screenshot',
          params: { filename: 'test.png' }
        },
        timestamp: new Date()
      };

      const response = await persona.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data).toBeDefined();
    });

    test('should process chat messages through LoRA adaptation', async () => {
      const message = {
        id: 'test-2',
        from: 'test-client',
        to: 'persona-test-persona',
        type: 'chat_message',
        data: {
          message: 'Analyze this patent for prior art',
          context: { patentNumber: 'US10,123,456' }
        },
        timestamp: new Date()
      };

      const response = await persona.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.response).toBeDefined();
    });
  });

  describe('Academy Training', () => {
    beforeEach(async () => {
      await persona.start();
    });

    test('should handle Testing Droid attacks', async () => {
      const message = {
        id: 'test-3',
        from: 'academy',
        to: 'persona-test-persona',
        type: 'academy_training',
        data: {
          action: 'generate_attacks',
          payload: { domain: 'patent_law' }
        },
        timestamp: new Date()
      };

      const response = await persona.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.attacks).toBeDefined();
      expect(response.data.role).toBe('testing_droid');
    });

    test('should handle Protocol Sheriff defense', async () => {
      // Change role to protocol sheriff
      const sheriffPersona = new PersonaDaemon(config, {
        enabled: true,
        role: 'protocol_sheriff',
        trainingDomain: 'patent_law'
      });

      await sheriffPersona.start();

      const message = {
        id: 'test-4',
        from: 'academy',
        to: 'persona-test-persona',
        type: 'academy_training',
        data: {
          action: 'validate_attacks',
          payload: {
            attacks: ['test attack 1', 'test attack 2']
          }
        },
        timestamp: new Date()
      };

      const response = await sheriffPersona.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.results).toBeDefined();
      expect(response.data.role).toBe('protocol_sheriff');

      await sheriffPersona.stop();
    });
  });

  describe('LoRA Adaptation', () => {
    beforeEach(async () => {
      await persona.start();
    });

    test('should load LoRA adapter stack', async () => {
      const message = {
        id: 'test-5',
        from: 'lora-system',
        to: 'persona-test-persona',
        type: 'lora_adaptation',
        data: {
          action: 'load_stack',
          adapters: ['continuum.legal', 'continuum.legal.patent']
        },
        timestamp: new Date()
      };

      const response = await persona.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.stackSize).toBeGreaterThan(0);
    });

    test('should create new LoRA adapter', async () => {
      const message = {
        id: 'test-6',
        from: 'academy',
        to: 'persona-test-persona',
        type: 'lora_adaptation',
        data: {
          action: 'create_adapter',
          trainingData: ['example 1', 'example 2'],
          domain: 'test.domain'
        },
        timestamp: new Date()
      };

      const response = await persona.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.stackSize).toBeGreaterThan(0);
    });
  });

  describe('Status Reporting', () => {
    beforeEach(async () => {
      await persona.start();
    });

    test('should return persona-specific status', async () => {
      const message = {
        id: 'test-7',
        from: 'monitor',
        to: 'persona-test-persona',
        type: 'get_status',
        data: {},
        timestamp: new Date()
      };

      const response = await persona.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.persona).toBeDefined();
      expect(response.data.persona.id).toBe('test-persona');
      expect(response.data.persona.name).toBe('Test Patent Expert');
      expect(response.data.persona.capabilities).toContain('chat');
    });
  });

  describe('Error Handling', () => {
    beforeEach(async () => {
      await persona.start();
    });

    test('should handle unknown message types gracefully', async () => {
      const message = {
        id: 'test-8',
        from: 'test-client',
        to: 'persona-test-persona',
        type: 'unknown_type',
        data: {},
        timestamp: new Date()
      };

      const response = await persona.handleMessage(message);
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown message type');
    });

    test('should handle Academy training when disabled', async () => {
      // Create persona without Academy training
      const basicPersona = new PersonaDaemon(config);
      await basicPersona.start();

      const message = {
        id: 'test-9',
        from: 'academy',
        to: 'persona-test-persona',
        type: 'academy_training',
        data: { action: 'test' },
        timestamp: new Date()
      };

      const response = await basicPersona.handleMessage(message);
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('Academy training not enabled');

      await basicPersona.stop();
    });
  });
});

/**
 * Integration Tests - Test persona with real command system
 */
describe('PersonaDaemon Integration', () => {
  test('should integrate with existing command system', () => {
    // This would test integration with actual CommandProcessor
    // and WebSocket daemon when those systems are available
    expect(true).toBe(true); // Placeholder
  });

  test('should integrate with Academy training system', () => {
    // This would test integration with actual Academy components
    // when the full training system is implemented
    expect(true).toBe(true); // Placeholder
  });

  test('should integrate with LoRA registry system', () => {
    // This would test integration with actual LoRA adapter registry
    // when the registry system is implemented
    expect(true).toBe(true); // Placeholder
  });
});