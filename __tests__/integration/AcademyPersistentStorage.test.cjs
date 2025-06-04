/**
 * Integration Tests for Academy Persistent Storage
 * Tests the integration between AcademyWebInterface and PersistentStorage
 */

const fs = require('fs');
const path = require('path');
const AcademyWebInterface = require('../../src/ui/AcademyWebInterface.cjs');

// Mock dependencies
const mockContinuum = {
  webSocketServer: {
    broadcast: jest.fn()
  }
};

// Test utilities
const TEST_STORAGE_DIR = path.join(__dirname, 'test-academy-storage');

describe('Academy Persistent Storage Integration', () => {
  let academyInterface;
  
  beforeEach(() => {
    // Clean up test storage directory
    if (fs.existsSync(TEST_STORAGE_DIR)) {
      fs.rmSync(TEST_STORAGE_DIR, { recursive: true, force: true });
    }
    
    // Create fresh Academy interface with test storage
    academyInterface = new AcademyWebInterface(mockContinuum);
    // Override storage path for testing
    academyInterface.storage.baseDir = TEST_STORAGE_DIR;
    academyInterface.storage.ensureDirectoryExists();
  });
  
  afterEach(() => {
    // Clean up test storage directory
    if (fs.existsSync(TEST_STORAGE_DIR)) {
      fs.rmSync(TEST_STORAGE_DIR, { recursive: true, force: true });
    }
  });

  describe('Training Session Persistence', () => {
    test('should persist failed training sessions', async () => {
      const personaName = 'TestPersona-Failed';
      const session = {
        personaName,
        specialization: 'test_specialization',
        status: 'training',
        startTime: new Date(),
        progress: 100,
        currentRound: 10,
        totalRounds: 10,
        graduationScore: 0.67, // Below 85% threshold
        logs: ['Training started', 'Training completed']
      };
      
      // Simulate training failure
      await academyInterface.failPersona(personaName, session);
      
      // Verify data was persisted
      const storageFile = path.join(TEST_STORAGE_DIR, 'academy-sessions.json');
      expect(fs.existsSync(storageFile)).toBe(true);
      
      // Load and verify persisted data
      const persistedData = JSON.parse(fs.readFileSync(storageFile, 'utf8'));
      expect(persistedData.completedPersonas[personaName]).toBeDefined();
      expect(persistedData.completedPersonas[personaName].status).toBe('failed');
      expect(persistedData.completedPersonas[personaName].graduationScore).toBe(0.67);
      expect(persistedData._metadata).toBeDefined();
    });

    test('should persist graduated training sessions', async () => {
      const personaName = 'TestPersona-Graduated';
      const session = {
        personaName,
        specialization: 'test_specialization',
        status: 'creating_adapter',
        startTime: new Date(),
        progress: 100,
        currentRound: 10,
        totalRounds: 10,
        graduationScore: 0.92, // Above 85% threshold
        logs: ['Training started', 'Training completed successfully']
      };
      
      // Simulate successful graduation
      await academyInterface.graduatePersona(personaName, session);
      
      // Verify data was persisted
      const storageFile = path.join(TEST_STORAGE_DIR, 'academy-sessions.json');
      expect(fs.existsSync(storageFile)).toBe(true);
      
      // Load and verify persisted data
      const persistedData = JSON.parse(fs.readFileSync(storageFile, 'utf8'));
      expect(persistedData.completedPersonas[personaName]).toBeDefined();
      expect(persistedData.completedPersonas[personaName].status).toBe('graduated');
      expect(persistedData.completedPersonas[personaName].graduationScore).toBe(0.92);
    });

    test('should load existing training sessions on startup', () => {
      // Create existing Academy data
      const existingData = {
        completedPersonas: {
          'ExistingPersona-1': {
            personaName: 'ExistingPersona-1',
            status: 'failed',
            startTime: '2025-01-03T10:00:00.000Z',
            completedAt: '2025-01-03T10:30:00.000Z',
            graduationScore: 0.73,
            logs: ['Training completed with failure']
          },
          'ExistingPersona-2': {
            personaName: 'ExistingPersona-2',
            status: 'graduated',
            startTime: '2025-01-03T11:00:00.000Z',
            completedAt: '2025-01-03T11:45:00.000Z',
            graduationScore: 0.91,
            logs: ['Training completed successfully']
          }
        },
        _metadata: {
          savedAt: '2025-01-03T12:00:00.000Z',
          source: 'TestData'
        }
      };
      
      // Save existing data
      const storageFile = path.join(TEST_STORAGE_DIR, 'academy-sessions.json');
      fs.writeFileSync(storageFile, JSON.stringify(existingData, null, 2));
      
      // Create new Academy interface (should load existing data)
      const newAcademyInterface = new AcademyWebInterface(mockContinuum);
      newAcademyInterface.storage.baseDir = TEST_STORAGE_DIR;
      newAcademyInterface.loadAcademyData();
      
      // Verify data was loaded
      expect(newAcademyInterface.completedPersonas.size).toBe(2);
      expect(newAcademyInterface.completedPersonas.has('ExistingPersona-1')).toBe(true);
      expect(newAcademyInterface.completedPersonas.has('ExistingPersona-2')).toBe(true);
      
      // Verify date objects were restored
      const persona1 = newAcademyInterface.completedPersonas.get('ExistingPersona-1');
      expect(persona1.startTime).toBeInstanceOf(Date);
      expect(persona1.completedAt).toBeInstanceOf(Date);
    });

    test('should handle multiple training sessions over time', async () => {
      const sessions = [
        {
          personaName: 'Persona-1',
          status: 'failed',
          graduationScore: 0.65
        },
        {
          personaName: 'Persona-2', 
          status: 'graduated',
          graduationScore: 0.89
        },
        {
          personaName: 'Persona-3',
          status: 'failed',
          graduationScore: 0.71
        }
      ];
      
      // Process multiple sessions
      for (const sessionData of sessions) {
        const session = {
          ...sessionData,
          specialization: 'test_spec',
          startTime: new Date(),
          progress: 100,
          currentRound: 10,
          totalRounds: 10,
          logs: [`${sessionData.personaName} training completed`]
        };
        
        if (sessionData.status === 'failed') {
          await academyInterface.failPersona(sessionData.personaName, session);
        } else {
          await academyInterface.graduatePersona(sessionData.personaName, session);
        }
      }
      
      // Verify all sessions were persisted
      expect(academyInterface.completedPersonas.size).toBe(3);
      
      // Verify storage file contains all sessions
      const storageFile = path.join(TEST_STORAGE_DIR, 'academy-sessions.json');
      const persistedData = JSON.parse(fs.readFileSync(storageFile, 'utf8'));
      expect(Object.keys(persistedData.completedPersonas)).toHaveLength(3);
      
      // Verify specific session data
      expect(persistedData.completedPersonas['Persona-1'].status).toBe('failed');
      expect(persistedData.completedPersonas['Persona-2'].status).toBe('graduated');
      expect(persistedData.completedPersonas['Persona-3'].status).toBe('failed');
    });
  });

  describe('Academy Status with Persistence', () => {
    test('should return correct status with persisted data', async () => {
      // Add some completed personas
      const failedSession = {
        personaName: 'Failed-Persona',
        status: 'training',
        graduationScore: 0.68,
        startTime: new Date(),
        logs: []
      };
      
      const graduatedSession = {
        personaName: 'Graduated-Persona', 
        status: 'creating_adapter',
        graduationScore: 0.94,
        startTime: new Date(),
        logs: []
      };
      
      await academyInterface.failPersona('Failed-Persona', failedSession);
      await academyInterface.graduatePersona('Graduated-Persona', graduatedSession);
      
      // Get Academy status
      const status = academyInterface.getAcademyStatus();
      
      expect(status.stats.totalPersonas).toBe(2);
      expect(status.stats.graduated).toBe(1);
      expect(status.stats.failed).toBe(1);
      expect(status.stats.activeTraining).toBe(0);
      expect(status.completed).toHaveLength(2);
      
      // Verify failed and graduated personas are in completed list
      const failedPersona = status.completed.find(p => p.personaName === 'Failed-Persona');
      const graduatedPersona = status.completed.find(p => p.personaName === 'Graduated-Persona');
      
      expect(failedPersona).toBeDefined();
      expect(failedPersona.status).toBe('failed');
      expect(graduatedPersona).toBeDefined();
      expect(graduatedPersona.status).toBe('graduated');
    });

    test('should limit completed personas in status response', async () => {
      // Create many completed personas (more than the limit of 5)
      for (let i = 0; i < 8; i++) {
        const session = {
          personaName: `TestPersona-${i}`,
          status: 'training',
          graduationScore: 0.70,
          startTime: new Date(Date.now() + i * 1000), // Different timestamps
          logs: []
        };
        
        await academyInterface.failPersona(`TestPersona-${i}`, session);
      }
      
      const status = academyInterface.getAcademyStatus();
      
      // Should have all 8 in total stats
      expect(status.stats.totalPersonas).toBe(8);
      expect(status.stats.failed).toBe(8);
      
      // But only last 5 in completed array
      expect(status.completed).toHaveLength(5);
      
      // Should be the most recent ones (TestPersona-3 through TestPersona-7)
      const completedNames = status.completed.map(p => p.personaName).sort();
      expect(completedNames).toEqual([
        'TestPersona-3', 'TestPersona-4', 'TestPersona-5', 'TestPersona-6', 'TestPersona-7'
      ]);
    });
  });

  describe('Error Handling and Edge Cases', () => {
    test('should handle corrupted storage file gracefully', () => {
      // Create corrupted storage file
      const storageFile = path.join(TEST_STORAGE_DIR, 'academy-sessions.json');
      fs.writeFileSync(storageFile, '{ invalid json content }');
      
      // Should not crash when loading
      expect(() => {
        const newAcademyInterface = new AcademyWebInterface(mockContinuum);
        newAcademyInterface.storage.baseDir = TEST_STORAGE_DIR;
        newAcademyInterface.loadAcademyData();
      }).not.toThrow();
    });

    test('should handle missing storage directory gracefully', () => {
      // Remove storage directory
      if (fs.existsSync(TEST_STORAGE_DIR)) {
        fs.rmSync(TEST_STORAGE_DIR, { recursive: true, force: true });
      }
      
      // Should create directory and continue normally
      expect(() => {
        new AcademyWebInterface(mockContinuum);
      }).not.toThrow();
    });

    test('should handle read-only storage directory', () => {
      // Create storage directory
      fs.mkdirSync(TEST_STORAGE_DIR, { recursive: true });
      
      // Make directory read-only (on systems that support it)
      try {
        fs.chmodSync(TEST_STORAGE_DIR, 0o444);
        
        const session = {
          personaName: 'ReadOnlyTest',
          status: 'training',
          graduationScore: 0.70,
          startTime: new Date(),
          logs: []
        };
        
        // Should handle save failure gracefully
        expect(async () => {
          await academyInterface.failPersona('ReadOnlyTest', session);
        }).not.toThrow();
        
      } finally {
        // Restore write permissions for cleanup
        fs.chmodSync(TEST_STORAGE_DIR, 0o755);
      }
    });
  });

  describe('Storage Performance', () => {
    test('should handle rapid successive saves efficiently', async () => {
      const startTime = Date.now();
      
      // Perform multiple rapid saves
      for (let i = 0; i < 10; i++) {
        const session = {
          personaName: `RapidTest-${i}`,
          status: 'training',
          graduationScore: 0.70,
          startTime: new Date(),
          logs: [`Rapid test ${i}`]
        };
        
        await academyInterface.failPersona(`RapidTest-${i}`, session);
      }
      
      const endTime = Date.now();
      const duration = endTime - startTime;
      
      // Should complete within reasonable time (less than 2 seconds)
      expect(duration).toBeLessThan(2000);
      
      // Verify all data was saved
      expect(academyInterface.completedPersonas.size).toBe(10);
    });
  });
});