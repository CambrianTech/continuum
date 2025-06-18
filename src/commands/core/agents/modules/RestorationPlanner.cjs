/**
 * RestorationPlanner - Archaeological restoration phase planning and guidance
 * Provides phase-by-phase restoration strategy with risk assessment
 */

class RestorationPlanner {
  constructor() {
    this.phases = this.getRestorationPhases();
  }

  /**
   * Get all restoration phases with metadata
   */
  getRestorationPhases() {
    return [
      {
        id: 'phase1',
        name: 'Phase 1: UI Renaissance',
        description: 'Restore Mass Effect-style slideout panels and agent selection',
        timeline: '2-4 hours',
        risk: 'Low',
        impact: 'High',
        prerequisites: [],
        components: [
          'AgentSelector.js',
          'SimpleAgentSelector.js', 
          'AgentSelectorUtils.js'
        ],
        startCommand: 'git show 4ffb32e:src/ui/components/AgentSelector.js > src/ui/components/AgentSelector.js',
        validation: 'python3 python-client/trust_the_process.py --screenshot',
        rollback: 'rm src/ui/components/AgentSelector.js src/ui/components/SimpleAgentSelector.js',
        successCriteria: [
          'Mass Effect slideout panels visible',
          'Agent selection dropdowns functional',
          'Glass morphism styling applied',
          'No console errors in browser'
        ]
      },
      {
        id: 'phase2',
        name: 'Phase 2: Academy Resurrection', 
        description: 'Restore Matrix-inspired adversarial AI training system',
        timeline: '4-8 hours',
        risk: 'Medium',
        impact: 'High',
        prerequisites: ['phase1'],
        components: [
          'Academy.cjs',
          'TestingDroid.cjs',
          'LoRAAdapter.cjs',
          'PersonaFactory.cjs',
          'ProtocolSheriff.cjs'
        ],
        startCommand: 'git show f0e2fb9:src/core/Academy.cjs > src/core/Academy.cjs',
        validation: 'python3 python-client/trust_the_process.py --academy',
        rollback: 'rm src/core/Academy.cjs src/core/TestingDroid.cjs',
        successCriteria: [
          'Academy enrollment system functional',
          'TestingDroid vs ProtocolSheriff battles working',
          'Graduation scoring system active',
          'Academy UI integration complete'
        ]
      },
      {
        id: 'phase3',
        name: 'Phase 3: Routing Revival',
        description: 'Restore intelligent agent selection and process management',
        timeline: '3-6 hours', 
        risk: 'Medium',
        impact: 'Medium',
        prerequisites: ['phase1', 'phase2'],
        components: [
          'intelligent-routing.cjs',
          'process-manager.cjs',
          'self-improving-router.cjs',
          'tmux-claude-pool.cjs'
        ],
        startCommand: 'git show 72c5684:src/core/intelligent-routing.cjs > src/core/intelligent-routing.cjs',
        validation: 'python3 python-client/trust_the_process.py --routing',
        rollback: 'rm src/core/intelligent-routing.cjs src/core/process-manager.cjs',
        successCriteria: [
          'Smart agent selection working',
          'Process lifecycle management active',
          'Multi-agent session handling',
          'Self-improving routing functional'
        ]
      },
      {
        id: 'phase4',
        name: 'Phase 4: Integration & Testing',
        description: 'Integrate all restored components and validate system',
        timeline: '2-4 hours',
        risk: 'Low',
        impact: 'High', 
        prerequisites: ['phase1', 'phase2', 'phase3'],
        components: [
          'CommandRegistry integration',
          'WebSocket message routing',
          'UI state synchronization'
        ],
        startCommand: 'python3 python-client/trust_the_process.py --full-integration',
        validation: 'python3 python-client/trust_the_process.py --comprehensive',
        rollback: 'git checkout HEAD -- src/commands/CommandRegistry.cjs',
        successCriteria: [
          'All phases working together',
          'No regression in existing functionality',
          'WebSocket communication stable',
          'UI updates reflect backend state'
        ]
      }
    ];
  }

  /**
   * Get phase by ID
   */
  getPhase(phaseId) {
    return this.phases.find(phase => phase.id === phaseId);
  }

  /**
   * Check if phase prerequisites are met
   */
  arePrerequisitesMet(phaseId, completedPhases = []) {
    const phase = this.getPhase(phaseId);
    if (!phase) return false;

    return phase.prerequisites.every(prereq => 
      completedPhases.includes(prereq)
    );
  }

  /**
   * Get next available phase based on completed phases
   */
  getNextAvailablePhase(completedPhases = []) {
    return this.phases.find(phase => 
      !completedPhases.includes(phase.id) &&
      this.arePrerequisitesMet(phase.id, completedPhases)
    );
  }

  /**
   * Get safety protocols for restoration
   */
  getSafetyProtocols() {
    return [
      {
        step: 'Pre-Restoration Validation',
        description: 'ALWAYS validate system before changes',
        command: 'python3 python-client/trust_the_process.py',
        critical: true
      },
      {
        step: 'Git Status Check',
        description: 'Ensure clean working directory',
        command: 'git status',
        critical: true
      },
      {
        step: 'Incremental Approach',
        description: 'ONE PHASE AT A TIME - never skip phases',
        command: null,
        critical: true
      },
      {
        step: 'Post-Change Validation',
        description: 'Validate after each restoration step',
        command: 'python3 python-client/trust_the_process.py',
        critical: true
      },
      {
        step: 'Rollback on Failure',
        description: 'Use phase-specific rollback commands if validation fails',
        command: 'See phase.rollback for specific commands',
        critical: true
      }
    ];
  }

  /**
   * Get archaeological discoveries status
   */
  getArchaeologicalStatus() {
    return {
      functional: [
        {
          name: 'Hierarchical LoRA System',
          status: 'FULLY FUNCTIONAL',
          description: 'Composable expertise architecture ready',
          location: 'src/adapters/HierarchicalAdapter.cjs'
        },
        {
          name: 'Global Competition Network',
          status: 'INFRASTRUCTURE READY', 
          description: 'Sharing and benchmarking system active',
          location: 'src/commands/core/share/ShareCommand.cjs'
        },
        {
          name: 'Academy Scoring System',
          status: 'WORKING',
          description: 'Performance metrics and graduation scoring',
          location: 'src/core/Academy.cjs'
        }
      ],
      recoverable: [
        {
          name: 'Mass Effect UI',
          status: 'Git recoverable',
          commits: ['4ffb32e', '41c02a2'],
          description: 'Slideout panels and agent selection',
          phase: 'phase1'
        },
        {
          name: 'Academy Training',
          status: 'Git recoverable', 
          commits: ['f0e2fb9'],
          description: 'TestingDroid vs ProtocolSheriff system',
          phase: 'phase2'
        },
        {
          name: 'Intelligent Routing',
          status: 'Git recoverable',
          commits: ['72c5684'], 
          description: 'Smart agent selection and process management',
          phase: 'phase3'
        }
      ]
    };
  }

  /**
   * Calculate restoration timeline estimate
   */
  calculateTotalTimeline(phases = null) {
    const targetPhases = phases || this.phases;
    let totalHours = 0;
    
    targetPhases.forEach(phase => {
      const hours = this.parseTimelineHours(phase.timeline);
      totalHours += hours;
    });
    
    return {
      totalHours,
      estimatedDays: Math.ceil(totalHours / 8), // 8 hour work days
      timeline: `${totalHours} hours (${Math.ceil(totalHours / 8)} days)`
    };
  }

  /**
   * Parse timeline string to hours
   */
  parseTimelineHours(timeline) {
    if (timeline.includes('hour')) {
      const match = timeline.match(/(\d+)-(\d+)/);
      if (match) {
        return (parseInt(match[1]) + parseInt(match[2])) / 2; // Average
      }
      return parseInt(timeline.match(/(\d+)/)?.[1] || '4');
    }
    return 8; // Default
  }

  /**
   * Get restoration recommendations based on current state
   */
  getRecommendations(currentPhase = null, agentExperience = 'novice') {
    const recommendations = [];
    
    if (!currentPhase) {
      recommendations.push({
        type: 'start',
        priority: 'high',
        message: 'Start with Phase 1: UI Renaissance - lowest risk, highest visual impact'
      });
    }
    
    if (agentExperience === 'novice') {
      recommendations.push({
        type: 'safety',
        priority: 'critical',
        message: 'Focus on validation: run trust_the_process.py before AND after each change'
      });
    }
    
    recommendations.push({
      type: 'strategy',
      priority: 'medium', 
      message: 'Read RESTORATION-STRATEGY.md for complete step-by-step guidance'
    });
    
    return recommendations;
  }
}

module.exports = RestorationPlanner;