/**
 * RestorationPlanner Unit Tests
 * Test restoration phase planning, prerequisites, and safety protocols
 */

const assert = require('assert');
const RestorationPlanner = require('../modules/RestorationPlanner.cjs');

describe('RestorationPlanner', () => {
  let planner;

  beforeEach(() => {
    planner = new RestorationPlanner();
  });

  describe('getRestorationPhases', () => {
    it('should return all restoration phases', () => {
      const phases = planner.getRestorationPhases();
      
      assert(phases.length >= 3); // At least UI, Academy, Routing phases
      
      phases.forEach(phase => {
        assert(typeof phase.id === 'string');
        assert(typeof phase.name === 'string');
        assert(typeof phase.description === 'string');
        assert(['Low', 'Medium', 'High'].includes(phase.risk));
        assert(['Low', 'Medium', 'High'].includes(phase.impact));
        assert(Array.isArray(phase.prerequisites));
        assert(Array.isArray(phase.components));
        assert(typeof phase.startCommand === 'string');
        assert(typeof phase.validation === 'string');
        assert(Array.isArray(phase.successCriteria));
      });
    });

    it('should have Phase 1 with no prerequisites', () => {
      const phases = planner.getRestorationPhases();
      const phase1 = phases.find(p => p.id === 'phase1');
      
      assert(phase1);
      assert.strictEqual(phase1.prerequisites.length, 0);
      assert.strictEqual(phase1.risk, 'Low');
    });

    it('should have proper prerequisite chains', () => {
      const phases = planner.getRestorationPhases();
      const phase2 = phases.find(p => p.id === 'phase2');
      const phase3 = phases.find(p => p.id === 'phase3');
      
      assert(phase2);
      assert(phase3);
      
      // Phase 2 should depend on Phase 1
      assert(phase2.prerequisites.includes('phase1'));
      
      // Phase 3 should depend on Phase 1 and 2
      assert(phase3.prerequisites.includes('phase1'));
      assert(phase3.prerequisites.includes('phase2'));
    });
  });

  describe('getPhase', () => {
    it('should return phase by ID', () => {
      const phase1 = planner.getPhase('phase1');
      
      assert(phase1);
      assert.strictEqual(phase1.id, 'phase1');
      assert(phase1.name.includes('UI Renaissance'));
    });

    it('should return undefined for invalid ID', () => {
      const phase = planner.getPhase('invalid');
      assert.strictEqual(phase, undefined);
    });
  });

  describe('arePrerequisitesMet', () => {
    it('should return true for phase with no prerequisites', () => {
      const met = planner.arePrerequisitesMet('phase1', []);
      assert.strictEqual(met, true);
    });

    it('should return true when prerequisites are completed', () => {
      const met = planner.arePrerequisitesMet('phase2', ['phase1']);
      assert.strictEqual(met, true);
    });

    it('should return false when prerequisites are missing', () => {
      const met = planner.arePrerequisitesMet('phase2', []);
      assert.strictEqual(met, false);
    });

    it('should return false for invalid phase', () => {
      const met = planner.arePrerequisitesMet('invalid', []);
      assert.strictEqual(met, false);
    });

    it('should handle multiple prerequisites', () => {
      const met1 = planner.arePrerequisitesMet('phase3', ['phase1']); // Missing phase2
      const met2 = planner.arePrerequisitesMet('phase3', ['phase1', 'phase2']); // All met
      
      assert.strictEqual(met1, false);
      assert.strictEqual(met2, true);
    });
  });

  describe('getNextAvailablePhase', () => {
    it('should return phase1 when no phases completed', () => {
      const next = planner.getNextAvailablePhase([]);
      
      assert(next);
      assert.strictEqual(next.id, 'phase1');
    });

    it('should return phase2 when phase1 completed', () => {
      const next = planner.getNextAvailablePhase(['phase1']);
      
      assert(next);
      assert.strictEqual(next.id, 'phase2');
    });

    it('should return phase3 when phase1 and phase2 completed', () => {
      const next = planner.getNextAvailablePhase(['phase1', 'phase2']);
      
      assert(next);
      assert.strictEqual(next.id, 'phase3');
    });

    it('should return undefined when all phases completed', () => {
      const allPhases = planner.getRestorationPhases().map(p => p.id);
      const next = planner.getNextAvailablePhase(allPhases);
      
      assert.strictEqual(next, undefined);
    });

    it('should skip phases that are already completed', () => {
      const next = planner.getNextAvailablePhase(['phase1', 'phase3']); // Skip phase2
      
      assert(next);
      assert.strictEqual(next.id, 'phase2'); // Should still return phase2 as it's available
    });
  });

  describe('getSafetyProtocols', () => {
    it('should return safety protocols array', () => {
      const protocols = planner.getSafetyProtocols();
      
      assert(Array.isArray(protocols));
      assert(protocols.length > 0);
      
      protocols.forEach(protocol => {
        assert(typeof protocol.step === 'string');
        assert(typeof protocol.description === 'string');
        assert(typeof protocol.critical === 'boolean');
        // command can be string or null
      });
    });

    it('should include critical validation steps', () => {
      const protocols = planner.getSafetyProtocols();
      
      const preValidation = protocols.find(p => p.step.includes('Pre-Restoration'));
      const postValidation = protocols.find(p => p.step.includes('Post-Change'));
      
      assert(preValidation);
      assert(postValidation);
      assert.strictEqual(preValidation.critical, true);
      assert.strictEqual(postValidation.critical, true);
    });

    it('should include git status check', () => {
      const protocols = planner.getSafetyProtocols();
      const gitCheck = protocols.find(p => p.step.includes('Git Status'));
      
      assert(gitCheck);
      assert.strictEqual(gitCheck.command, 'git status');
    });
  });

  describe('getArchaeologicalStatus', () => {
    it('should return functional and recoverable components', () => {
      const status = planner.getArchaeologicalStatus();
      
      assert(status.functional);
      assert(status.recoverable);
      assert(Array.isArray(status.functional));
      assert(Array.isArray(status.recoverable));
    });

    it('should have well-formed functional components', () => {
      const status = planner.getArchaeologicalStatus();
      
      status.functional.forEach(component => {
        assert(typeof component.name === 'string');
        assert(typeof component.status === 'string');
        assert(typeof component.description === 'string');
        assert(typeof component.location === 'string');
      });
    });

    it('should have well-formed recoverable components', () => {
      const status = planner.getArchaeologicalStatus();
      
      status.recoverable.forEach(component => {
        assert(typeof component.name === 'string');
        assert(typeof component.status === 'string');
        assert(Array.isArray(component.commits));
        assert(typeof component.description === 'string');
        assert(typeof component.phase === 'string');
      });
    });

    it('should include hierarchical LoRA system as functional', () => {
      const status = planner.getArchaeologicalStatus();
      const loraSystem = status.functional.find(c => c.name.includes('LoRA'));
      
      assert(loraSystem);
      assert.strictEqual(loraSystem.status, 'FULLY FUNCTIONAL');
    });
  });

  describe('calculateTotalTimeline', () => {
    it('should calculate total hours and days', () => {
      const timeline = planner.calculateTotalTimeline();
      
      assert(typeof timeline.totalHours === 'number');
      assert(typeof timeline.estimatedDays === 'number');
      assert(typeof timeline.timeline === 'string');
      assert(timeline.totalHours > 0);
      assert(timeline.estimatedDays > 0);
    });

    it('should handle subset of phases', () => {
      const phases = planner.getRestorationPhases().slice(0, 2); // First 2 phases
      const timeline = planner.calculateTotalTimeline(phases);
      
      assert(timeline.totalHours > 0);
      assert(timeline.totalHours < 50); // Should be reasonable for 2 phases
    });
  });

  describe('parseTimelineHours', () => {
    it('should parse hour ranges', () => {
      assert.strictEqual(planner.parseTimelineHours('2-4 hours'), 3); // Average
      assert.strictEqual(planner.parseTimelineHours('6 hours'), 6);
    });

    it('should default to 8 for non-hour timelines', () => {
      assert.strictEqual(planner.parseTimelineHours('1 day'), 8);
      assert.strictEqual(planner.parseTimelineHours('unknown'), 8);
    });
  });

  describe('getRecommendations', () => {
    it('should provide start recommendation for no current phase', () => {
      const recommendations = planner.getRecommendations(null, 'novice');
      
      const startRec = recommendations.find(r => r.type === 'start');
      assert(startRec);
      assert.strictEqual(startRec.priority, 'high');
    });

    it('should provide safety recommendations for novice agents', () => {
      const recommendations = planner.getRecommendations(null, 'novice');
      
      const safetyRec = recommendations.find(r => r.type === 'safety');
      assert(safetyRec);
      assert.strictEqual(safetyRec.priority, 'critical');
    });

    it('should always include strategy recommendation', () => {
      const recommendations = planner.getRecommendations('phase1', 'expert');
      
      const strategyRec = recommendations.find(r => r.type === 'strategy');
      assert(strategyRec);
      assert(strategyRec.message.includes('RESTORATION-STRATEGY.md'));
    });

    it('should handle different experience levels', () => {
      const noviceRecs = planner.getRecommendations(null, 'novice');
      const expertRecs = planner.getRecommendations(null, 'expert');
      
      // Both should have recommendations, but novice should have more safety focus
      assert(noviceRecs.length > 0);
      assert(expertRecs.length > 0);
      
      const noviceSafety = noviceRecs.filter(r => r.type === 'safety');
      const expertSafety = expertRecs.filter(r => r.type === 'safety');
      
      assert(noviceSafety.length >= expertSafety.length);
    });
  });
});

// Run tests if this file is executed directly
if (require.main === module) {
  console.log('🧪 Running RestorationPlanner tests...\n');
  
  try {
    const testPlanner = new RestorationPlanner();
    
    // Test phase retrieval
    const phases = testPlanner.getRestorationPhases();
    console.log(`✅ Retrieved ${phases.length} restoration phases`);
    
    // Test prerequisites
    const phase1Available = testPlanner.arePrerequisitesMet('phase1', []);
    const phase2Available = testPlanner.arePrerequisitesMet('phase2', []);
    console.log(`✅ Phase 1 available: ${phase1Available}, Phase 2 available: ${phase2Available}`);
    
    // Test next available phase
    const nextPhase = testPlanner.getNextAvailablePhase([]);
    console.log(`✅ Next available phase: ${nextPhase.id}`);
    
    // Test safety protocols
    const protocols = testPlanner.getSafetyProtocols();
    console.log(`✅ Retrieved ${protocols.length} safety protocols`);
    
    // Test archaeological status
    const status = testPlanner.getArchaeologicalStatus();
    console.log(`✅ Archaeological status: ${status.functional.length} functional, ${status.recoverable.length} recoverable`);
    
    // Test timeline calculation
    const timeline = testPlanner.calculateTotalTimeline();
    console.log(`✅ Total timeline: ${timeline.timeline}`);
    
    // Test recommendations
    const recommendations = testPlanner.getRecommendations(null, 'novice');
    console.log(`✅ Generated ${recommendations.length} recommendations for novice agent`);
    
    console.log('\n🎉 All RestorationPlanner tests passed!');
    
  } catch (error) {
    console.error('❌ RestorationPlanner test failed:', error.message);
    process.exit(1);
  }
}

module.exports = { RestorationPlanner };