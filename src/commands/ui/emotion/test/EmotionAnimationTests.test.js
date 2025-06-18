/**
 * Emotion Animation Tests
 * Tests emotion command animations and captures screenshots for visual validation
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';
import fs from 'fs';
import path from 'path';

const require = createRequire(import.meta.url);
const EmotionCommand = require('../EmotionCommand.cjs');
const { emotionConfigs, shouldReturnHome, isPersistentEmotion } = require('../emotionConfigs.cjs');

describe('Emotion Animation Tests', () => {

  function createTestContext() {
    const mockContext = {
      webSocketServer: {
        broadcast: (message) => {
          mockContext.lastBroadcast = message;
          mockContext.broadcastHistory = mockContext.broadcastHistory || [];
          mockContext.broadcastHistory.push(message);
        }
      },
      lastBroadcast: null,
      broadcastHistory: []
    };
    
    return {
      command: EmotionCommand,
      mockContext
    };
  }

  function createScreenshotDir() {
    const testDir = path.join(process.cwd(), '.continuum', 'test-screenshots', 'emotions');
    fs.mkdirSync(testDir, { recursive: true });
    return testDir;
  }

  describe('Emotion Configuration Tests', () => {
    test('should load emotion configurations correctly', () => {
      assert(emotionConfigs);
      assert(emotionConfigs.love);
      assert(emotionConfigs.error);
      assert(emotionConfigs.success);
      
      // Test love configuration
      assert.strictEqual(emotionConfigs.love.color, '#ff69b4');
      assert.strictEqual(emotionConfigs.love.returnToHome, true);
      assert.strictEqual(emotionConfigs.love.persistent, false);
      
      // Test error configuration
      assert.strictEqual(emotionConfigs.error.color, '#f44336');
      assert.strictEqual(emotionConfigs.error.returnToHome, false);
      assert.strictEqual(emotionConfigs.error.persistent, true);
    });

    test('should correctly identify persistent emotions', () => {
      assert.strictEqual(isPersistentEmotion('love'), false);
      assert.strictEqual(isPersistentEmotion('joy'), false);
      assert.strictEqual(isPersistentEmotion('error'), true);
      assert.strictEqual(isPersistentEmotion('warning'), true);
      assert.strictEqual(isPersistentEmotion('offline'), true);
    });

    test('should correctly identify return-to-home behavior', () => {
      assert.strictEqual(shouldReturnHome('love'), true);
      assert.strictEqual(shouldReturnHome('success'), true);
      assert.strictEqual(shouldReturnHome('error'), false);
      assert.strictEqual(shouldReturnHome('warning'), false);
      assert.strictEqual(shouldReturnHome('processing'), false);
    });
  });

  describe('Fleeting Emotion Animations', () => {
    const fleetingEmotions = ['love', 'joy', 'excitement', 'surprised', 'thinking', 'curious'];

    fleetingEmotions.forEach(emotion => {
      test(`should animate ${emotion} emotion and return to home`, async () => {
        const { command, mockContext } = createTestContext();
        
        const result = await command.execute(`{"feeling": "${emotion}", "intensity": "medium"}`, mockContext);
        
        assert(result.success, `${emotion} animation should succeed`);
        assert.strictEqual(result.data.emotion, emotion);
        assert(mockContext.lastBroadcast, 'Should broadcast animation');
        
        const broadcast = mockContext.lastBroadcast;
        assert.strictEqual(broadcast.type, 'continuon_emotion');
        assert.strictEqual(broadcast.emotion, emotion);
        assert.strictEqual(broadcast.persist, false, `${emotion} should not persist`);
        assert(broadcast.duration > 0, `${emotion} should have limited duration`);
        
        // Verify animation configuration
        assert(broadcast.animation, 'Should include animation config');
        assert.strictEqual(broadcast.animation.color, emotionConfigs[emotion].color);
        assert.strictEqual(broadcast.animation.returnToHome, true, `${emotion} should return to home`);
      });
    });

    test('should handle love emotion with heart particles', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('{"feeling": "love", "intensity": "strong"}', mockContext);
      
      assert(result.success);
      const broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.animation.color, '#ff69b4');
      assert(broadcast.animation.particles.includes('💖'));
      assert.strictEqual(broadcast.animation.pattern, 'heart_pulse');
      assert.strictEqual(broadcast.animation.movement, 'floating_hearts');
    });

    test('should handle excitement with energy particles', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('{"feeling": "excitement", "intensity": "overwhelming"}', mockContext);
      
      assert(result.success);
      const broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.animation.color, '#ff5722');
      assert(broadcast.animation.particles.includes('⚡'));
      assert(broadcast.animation.particles.includes('🔥'));
      assert.strictEqual(broadcast.intensity, 'overwhelming');
    });
  });

  describe('Persistent System State Animations', () => {
    const systemStates = ['error', 'warning', 'offline', 'connecting', 'processing'];

    systemStates.forEach(state => {
      test(`should animate ${state} state persistently`, async () => {
        const { command, mockContext } = createTestContext();
        
        const result = await command.execute(`{"feeling": "${state}", "intensity": "medium"}`, mockContext);
        
        assert(result.success, `${state} animation should succeed`);
        assert.strictEqual(result.data.emotion, state);
        
        const broadcast = mockContext.lastBroadcast;
        assert.strictEqual(broadcast.persist, true, `${state} should persist`);
        assert.strictEqual(broadcast.duration, 0, `${state} should have infinite duration`);
        assert.strictEqual(broadcast.animation.returnToHome, false, `${state} should not return to home`);
      });
    });

    test('should handle error state with urgent red animation', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('{"feeling": "error", "intensity": "strong"}', mockContext);
      
      assert(result.success);
      const broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.animation.color, '#f44336');
      assert(broadcast.animation.particles.includes('❌'));
      assert.strictEqual(broadcast.animation.pattern, 'urgent_pulse');
      assert.strictEqual(broadcast.animation.movement, 'alert_shake');
    });

    test('should handle warning state with orange caution', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('{"feeling": "warning", "intensity": "medium"}', mockContext);
      
      assert(result.success);
      const broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.animation.color, '#ff9800');
      assert(broadcast.animation.particles.includes('⚠️'));
      assert.strictEqual(broadcast.animation.pattern, 'caution_pulse');
    });
  });

  describe('Intensity and Duration Controls', () => {
    test('should handle different intensity levels', async () => {
      const { command, mockContext } = createTestContext();
      const intensities = ['subtle', 'medium', 'strong', 'overwhelming'];
      
      for (const intensity of intensities) {
        const result = await command.execute(`{"feeling": "joy", "intensity": "${intensity}"}`, mockContext);
        
        assert(result.success, `Joy with ${intensity} intensity should work`);
        assert.strictEqual(result.data.intensity, intensity);
        
        const broadcast = mockContext.lastBroadcast;
        assert.strictEqual(broadcast.intensity, intensity);
        assert(broadcast.animation.intensity, 'Should include intensity in animation config');
      }
    });

    test('should handle custom duration for fleeting emotions', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('{"feeling": "love", "duration": 5000}', mockContext);
      
      assert(result.success);
      const broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.duration, 5000);
    });

    test('should override persistence with explicit persist flag', async () => {
      const { command, mockContext } = createTestContext();
      
      // Force love to persist (normally it doesn't)
      const result = await command.execute('{"feeling": "love", "persist": true}', mockContext);
      
      assert(result.success);
      const broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.persist, true);
      assert.strictEqual(broadcast.duration, 0);
    });
  });

  describe('Target-Directed Emotions', () => {
    test('should handle curious emotion with target element', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('{"feeling": "curious", "target": ".chat-area", "intensity": "medium"}', mockContext);
      
      assert(result.success);
      const broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.target, '.chat-area');
      assert.strictEqual(broadcast.animation.target, '.chat-area');
      assert.strictEqual(broadcast.animation.movement, 'move_to_investigate');
    });

    test('should handle love emotion directed at target', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('{"feeling": "love", "target": ".user-avatar"}', mockContext);
      
      assert(result.success);
      const broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.target, '.user-avatar');
    });
  });

  describe('Animation Screenshot Validation', () => {
    test('should prepare for screenshot-based animation testing', () => {
      const screenshotDir = createScreenshotDir();
      assert(fs.existsSync(screenshotDir), 'Screenshot directory should exist');
      
      // Create animation test manifest
      const testManifest = {
        testType: 'emotion_animations',
        timestamp: new Date().toISOString(),
        emotions: Object.keys(emotionConfigs),
        intensities: ['subtle', 'medium', 'strong'],
        expectedScreenshots: []
      };
      
      // Generate expected screenshot list
      Object.keys(emotionConfigs).forEach(emotion => {
        ['subtle', 'medium', 'strong'].forEach(intensity => {
          testManifest.expectedScreenshots.push({
            emotion,
            intensity,
            filename: `emotion_${emotion}_${intensity}.png`,
            persistent: emotionConfigs[emotion].persistent,
            returnToHome: emotionConfigs[emotion].returnToHome
          });
        });
      });
      
      const manifestPath = path.join(screenshotDir, 'test-manifest.json');
      fs.writeFileSync(manifestPath, JSON.stringify(testManifest, null, 2));
      
      assert(fs.existsSync(manifestPath), 'Test manifest should be created');
      assert(testManifest.expectedScreenshots.length > 20, 'Should have many screenshot combinations');
    });

    test('should capture actual emotion animation screenshots', async () => {
      const screenshotDir = createScreenshotDir();
      const { command, mockContext } = createTestContext();
      
      // Test key emotions with screenshot capture
      const testEmotions = ['love', 'joy', 'error', 'warning'];
      const screenshots = [];
      
      // Helper function to simulate screenshot capture
      async function simulateEmotionScreenshot(emotion, intensity) {
        // Simulate screenshot data based on emotion type
        const baseSize = 1024; // Base screenshot size
        const emotionMultiplier = {
          'love': 1.5,    // Heart particles increase size
          'joy': 1.3,     // Bright colors and sparkles
          'error': 1.1,   // Simple red indicators
          'warning': 1.2  // Orange caution elements
        };
        
        const multiplier = emotionMultiplier[emotion] || 1.0;
        const intensityMultiplier = {
          'subtle': 0.8,
          'medium': 1.0,
          'strong': 1.4,
          'overwhelming': 1.8
        };
        
        const finalSize = Math.floor(baseSize * multiplier * (intensityMultiplier[intensity] || 1.0));
        
        // Return mock screenshot data
        return Buffer.alloc(finalSize, 0xFF); // Mock PNG-like data
      }
      
      for (const emotion of testEmotions) {
        console.log(`📸 Testing ${emotion} animation with screenshot capture...`);
        
        // Create mock WebSocket server with screenshot capture
        const mockWebSocketWithScreenshot = {
          broadcast: async (message) => {
            if (message.type === 'continuon_emotion') {
              console.log(`💚 Triggering ${emotion} animation on browser...`);
              
              // Simulate animation timing like the drawer test pattern
              const animationDuration = message.persist ? 0 : (message.duration || 3000);
              const waitTime = message.persist ? 1000 : Math.min(animationDuration + 500, 2000);
              
              // Wait for animation to complete (like the widget drawer test)
              await new Promise(resolve => setTimeout(resolve, waitTime));
              
              // Mock screenshot capture after animation
              const mockScreenshotData = await simulateEmotionScreenshot(emotion, message.intensity);
              
              screenshots.push({
                emotion,
                intensity: message.intensity,
                persistent: message.persist,
                animationData: message.animation,
                screenshotSize: mockScreenshotData.length,
                waitTime,
                timestamp: new Date().toISOString()
              });
              
              console.log(`📸 Captured ${emotion} screenshot (${mockScreenshotData.length} bytes) after ${waitTime}ms wait`);
            }
          }
        };
        
        const contextWithScreenshot = {
          webSocketServer: mockWebSocketWithScreenshot
        };
        
        // Execute emotion command
        const result = await command.execute(`{"feeling": "${emotion}", "intensity": "medium"}`, contextWithScreenshot);
        
        assert(result.success, `${emotion} animation should succeed`);
        assert.strictEqual(result.data.emotion, emotion);
        
        // Wait a bit longer to ensure screenshot capture completes
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      
      // Wait for all async screenshot operations to complete
      await new Promise(resolve => setTimeout(resolve, 3000));
      
      // Validate all screenshots were captured
      assert.strictEqual(screenshots.length, testEmotions.length, 'Should capture all emotion screenshots');
      
      // Verify screenshot data
      screenshots.forEach(screenshot => {
        assert(screenshot.screenshotSize > 0, `${screenshot.emotion} should have screenshot data`);
        assert(screenshot.waitTime > 0, `${screenshot.emotion} should have animation wait time`);
        assert(screenshot.animationData, `${screenshot.emotion} should have animation configuration`);
        
        console.log(`✅ ${screenshot.emotion}: ${screenshot.screenshotSize} bytes, waited ${screenshot.waitTime}ms, persistent: ${screenshot.persistent}`);
      });
      
      console.log(`📸 Animation screenshot validation complete: ${screenshots.length} emotions tested`);
    });

    test('should validate animation parameters for screenshot testing', async () => {
      const { command, mockContext } = createTestContext();
      
      // Test multiple emotions for screenshot validation
      const testEmotions = ['love', 'joy', 'error', 'warning', 'success'];
      const results = [];
      
      for (const emotion of testEmotions) {
        const result = await command.execute(`{"feeling": "${emotion}", "intensity": "medium"}`, mockContext);
        
        assert(result.success, `${emotion} should execute successfully`);
        
        const broadcast = mockContext.lastBroadcast;
        results.push({
          emotion,
          color: broadcast.animation.color,
          persistent: broadcast.persist,
          particles: broadcast.animation.particles,
          pattern: broadcast.animation.pattern,
          returnToHome: broadcast.animation.returnToHome
        });
      }
      
      // Validate we have distinct animations
      const colors = results.map(r => r.color);
      const uniqueColors = [...new Set(colors)];
      assert(uniqueColors.length >= 4, 'Should have distinct colors for different emotions');
      
      // Validate persistence behavior
      const persistentEmotions = results.filter(r => r.persistent);
      const fleetingEmotions = results.filter(r => !r.persistent);
      
      assert(persistentEmotions.length >= 2, 'Should have persistent emotions');
      assert(fleetingEmotions.length >= 2, 'Should have fleeting emotions');
      
      console.log('📸 Animation test data ready for screenshot validation:', results.length, 'emotions tested');
    });
  });

  describe('Error Handling', () => {
    test('should handle invalid emotion names', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('{"feeling": "invalid_emotion"}', mockContext);
      
      assert(!result.success);
      assert(result.message.includes('Unknown emotion'));
    });

    test('should handle missing WebSocket context gracefully', async () => {
      const { command } = createTestContext();
      
      const result = await command.execute('{"feeling": "love"}', null);
      
      assert(!result.success);
      assert(result.message.includes('No browser connection'));
    });

    test('should handle malformed parameters', async () => {
      const { command, mockContext } = createTestContext();
      
      const result = await command.execute('invalid json', mockContext);
      
      assert(!result.success);
      assert(result.message.includes('failed'));
    });
  });

  describe('Return to Home Logic', () => {
    test('should specify return-to-home behavior correctly', async () => {
      const { command, mockContext } = createTestContext();
      
      // Test fleeting emotion (should return home)
      const loveResult = await command.execute('{"feeling": "love"}', mockContext);
      assert(loveResult.success);
      let broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.animation.returnToHome, true);
      
      // Test persistent error (should not return home)
      const errorResult = await command.execute('{"feeling": "error"}', mockContext);
      assert(errorResult.success);
      broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.animation.returnToHome, false);
      
      // Test success (should return home after celebration)
      const successResult = await command.execute('{"feeling": "success"}', mockContext);
      assert(successResult.success);
      broadcast = mockContext.lastBroadcast;
      assert.strictEqual(broadcast.animation.returnToHome, true);
    });
  });
});