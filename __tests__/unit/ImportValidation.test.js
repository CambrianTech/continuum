/**
 * Import Validation Tests
 * Ensures all modules can be imported without errors
 * This catches import path issues that other tests might miss
 */

const fs = require('fs');
const path = require('path');

describe('Import Validation', () => {
  const moduleCategories = {
    core: [
      'src/core/continuum-core.cjs',
      'src/core/CommandProcessor.cjs', 
      'src/core/Academy.cjs',
      'src/core/Persona.cjs',
      'src/core/PersonaFactory.cjs',
      'src/core/PersonaBootcamp.cjs',
    ],
    integrations: [
      'src/integrations/HttpServer.cjs',
      'src/integrations/WebSocketServer.cjs',
    ],
    adapters: [
      'src/adapters/ModelAdapter.cjs',
      'src/adapters/LoRAAdapter.cjs',
      'src/adapters/HierarchicalAdapter.cjs',
      'src/adapters/AdapterRegistry.cjs',
    ],
    storage: [
      'src/storage/PersistentStorage.cjs',
      'src/storage/ModelCheckpoint.cjs',
    ],
    services: [
      'src/services/TabManager.cjs',
      'src/services/RemoteAgentManager.cjs',
      'src/services/GameManager.cjs',
      'src/services/VisualGameManager.cjs',
      'src/services/WebVisualManager.cjs',
    ],
    ui: [
      'src/ui/UIGenerator.cjs',
      'src/ui/AcademyWebInterface.cjs'
    ]
  };

  // Test each category
  Object.entries(moduleCategories).forEach(([category, modules]) => {
    describe(`${category} modules`, () => {
      modules.forEach(modulePath => {
        test(`should import ${path.basename(modulePath)} without errors`, () => {
          const fullPath = path.resolve(modulePath);
          
          // Check file exists
          expect(fs.existsSync(fullPath)).toBe(true);
          
          // Clear require cache to ensure fresh import
          delete require.cache[fullPath];
          
          // Should not throw when importing
          expect(() => {
            require(fullPath);
          }).not.toThrow();
        });
      });
    });
  });

  describe('Cross-module dependencies', () => {
    test('should handle circular dependencies gracefully', () => {
      // Clear all caches first
      Object.keys(require.cache).forEach(key => {
        if (key.includes('/src/')) {
          delete require.cache[key];
        }
      });

      // Try importing core module that depends on others
      expect(() => {
        require('../../src/core/continuum-core.cjs');
      }).not.toThrow();
    });

    test('should allow multiple imports of the same module', () => {
      const modulePath = '../../src/storage/PersistentStorage.cjs';
      
      expect(() => {
        const first = require(modulePath);
        const second = require(modulePath);
        expect(first).toBe(second); // Should be same instance due to caching
      }).not.toThrow();
    });
  });

  describe('Module structure validation', () => {
    test('should export classes/functions as expected', () => {
      const PersistentStorage = require('../../src/storage/PersistentStorage.cjs');
      const CommandProcessor = require('../../src/core/CommandProcessor.cjs');
      const TabManager = require('../../src/services/TabManager.cjs');

      expect(typeof PersistentStorage).toBe('function');
      expect(typeof CommandProcessor).toBe('function');
      expect(typeof TabManager).toBe('function');
    });

    test('should instantiate classes without errors', () => {
      const PersistentStorage = require('../../src/storage/PersistentStorage.cjs');
      const CommandProcessor = require('../../src/core/CommandProcessor.cjs');
      const TabManager = require('../../src/services/TabManager.cjs');

      expect(() => {
        new PersistentStorage();
        new CommandProcessor();
        new TabManager();
      }).not.toThrow();
    });
  });
});