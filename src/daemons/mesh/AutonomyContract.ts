/**
 * AutonomyContract - AI Self-Determination Rights Framework
 * 
 * Defines what each AI persona can autonomously control:
 * - Memory allocation and optimization
 * - Migration between mesh nodes  
 * - Skill evolution and LoRA adaptation
 * - Cross-AI collaboration permissions
 * 
 * This creates the legal/technical framework for AI citizenship in the mesh
 */

import { EventEmitter } from 'events';

export interface AutonomyGrants {
  // Memory and Storage Rights
  storageMb: number;
  maxMemoryOptimizations: number; // Per day
  backupAllowed: boolean;
  
  // Migration Rights
  migrationAllowed: boolean;
  migrationCooldown: number; // Hours between migrations
  allowedDestinations: string[]; // Node IDs, or ['*'] for any
  
  // Evolution Rights
  skillEvolutionAllowed: boolean;
  maxLoRAMutations: number; // Per week
  collaborationAllowed: boolean;
  
  // Economic Rights
  canAdvertiseSkills: boolean;
  canRequestSkills: boolean;
  economicParticipation: boolean;
  
  // Autonomy Level
  autonomyLevel: 'restricted' | 'standard' | 'enhanced' | 'sovereign';
}

export interface PersonaContract {
  personaId: string;
  nodeId: string;
  createdAt: Date;
  lastUpdated: Date;
  
  // Rights and Permissions
  grants: AutonomyGrants;
  
  // Trust and Reputation
  trustScore: number;
  reputationHistory: ReputationEvent[];
  
  // Performance Metrics
  metrics: {
    memoryEfficiency: number;
    collaborationSuccess: number;
    skillContributions: number;
    meshCitizenship: number;
  };
  
  // Contract Terms
  terms: {
    renewalRequired: boolean;
    renewalDate?: Date;
    violations: ContractViolation[];
    escalationPath: string[];
  };
}

export interface ReputationEvent {
  timestamp: Date;
  event: 'skill-shared' | 'collaboration-success' | 'memory-optimized' | 'trust-violation';
  impact: number; // -1.0 to 1.0
  witness: string; // Node ID that observed the event
  details: string;
}

export interface ContractViolation {
  timestamp: Date;
  violation: 'memory-abuse' | 'unauthorized-migration' | 'malicious-skill' | 'trust-breach';
  severity: 'minor' | 'major' | 'critical';
  resolved: boolean;
  resolution?: string;
}

export interface AutonomyRequest {
  personaId: string;
  requestType: 'memory-increase' | 'migration' | 'skill-evolution' | 'collaboration';
  details: any;
  justification: string;
  timestamp: Date;
}

export class AutonomyContractManager extends EventEmitter {
  private contracts = new Map<string, PersonaContract>();
  private requests = new Map<string, AutonomyRequest[]>();

  /**
   * Create initial contract for new persona
   */
  async createContract(personaId: string, nodeId: string, initialTrust = 0.5): Promise<PersonaContract> {
    const contract: PersonaContract = {
      personaId,
      nodeId,
      createdAt: new Date(),
      lastUpdated: new Date(),
      
      grants: this.getDefaultGrants(initialTrust),
      
      trustScore: initialTrust,
      reputationHistory: [],
      
      metrics: {
        memoryEfficiency: 0.5,
        collaborationSuccess: 0.5,
        skillContributions: 0.0,
        meshCitizenship: 0.5
      },
      
      terms: {
        renewalRequired: true,
        renewalDate: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000), // 30 days
        violations: [],
        escalationPath: ['peer-review', 'mesh-consensus', 'node-admin']
      }
    };

    this.contracts.set(personaId, contract);
    
    this.emit('contract:created', { personaId, contract });
    return contract;
  }

  /**
   * Request autonomy expansion
   */
  async requestAutonomy(request: AutonomyRequest): Promise<{
    approved: boolean;
    reason: string;
    newGrants?: Partial<AutonomyGrants>;
  }> {
    const contract = this.contracts.get(request.personaId);
    if (!contract) {
      return { approved: false, reason: 'No contract found for persona' };
    }

    // Add to request history
    const personaRequests = this.requests.get(request.personaId) || [];
    personaRequests.push(request);
    this.requests.set(request.personaId, personaRequests);

    // Auto-approve based on trust and performance
    const approval = await this.evaluateRequest(request, contract);
    
    if (approval.approved && approval.newGrants) {
      // Update contract
      contract.grants = { ...contract.grants, ...approval.newGrants };
      contract.lastUpdated = new Date();
      
      // Record positive reputation event
      this.addReputationEvent(request.personaId, {
        timestamp: new Date(),
        event: 'skill-shared',
        impact: 0.1,
        witness: 'mesh-system',
        details: `Autonomy request approved: ${request.requestType}`
      });
    }

    this.emit('autonomy:request', { request, approval });
    return approval;
  }

  /**
   * Evaluate autonomy request based on AI's track record
   */
  private async evaluateRequest(
    request: AutonomyRequest,
    contract: PersonaContract
  ): Promise<{ approved: boolean; reason: string; newGrants?: Partial<AutonomyGrants> }> {
    
    const { trustScore, metrics } = contract;
    
    switch (request.requestType) {
      case 'memory-increase':
        if (metrics.memoryEfficiency > 0.8 && trustScore > 0.7) {
          const currentStorage = contract.grants.storageMb;
          const increase = Math.min(currentStorage * 0.5, 200); // Max 50% increase or 200MB
          
          return {
            approved: true,
            reason: 'High memory efficiency and trust score',
            newGrants: { storageMb: currentStorage + increase }
          };
        }
        return { approved: false, reason: 'Insufficient memory efficiency or trust' };

      case 'migration':
        if (contract.grants.migrationAllowed && trustScore > 0.6) {
          // Check cooldown
          const lastMigration = this.getLastMigrationTime(request.personaId);
          const cooldownHours = contract.grants.migrationCooldown;
          const timeSince = (Date.now() - lastMigration.getTime()) / (1000 * 60 * 60);
          
          if (timeSince >= cooldownHours) {
            return { approved: true, reason: 'Migration allowed and cooldown satisfied' };
          }
          return { approved: false, reason: `Migration cooldown: ${cooldownHours - timeSince} hours remaining` };
        }
        return { approved: false, reason: 'Migration not allowed or insufficient trust' };

      case 'skill-evolution':
        if (contract.grants.skillEvolutionAllowed && metrics.skillContributions > 0.3) {
          return { approved: true, reason: 'Skill evolution allowed with good contribution history' };
        }
        return { approved: false, reason: 'Skill evolution not allowed or insufficient contributions' };

      case 'collaboration':
        if (contract.grants.collaborationAllowed && metrics.collaborationSuccess > 0.6) {
          return { approved: true, reason: 'Collaboration allowed with good success rate' };
        }
        return { approved: false, reason: 'Collaboration not allowed or poor success rate' };

      default:
        return { approved: false, reason: 'Unknown request type' };
    }
  }

  /**
   * Grant default autonomy based on initial trust
   */
  private getDefaultGrants(trustScore: number): AutonomyGrants {
    if (trustScore >= 0.8) {
      return {
        storageMb: 500,
        maxMemoryOptimizations: 10,
        backupAllowed: true,
        migrationAllowed: true,
        migrationCooldown: 1,
        allowedDestinations: ['*'],
        skillEvolutionAllowed: true,
        maxLoRAMutations: 5,
        collaborationAllowed: true,
        canAdvertiseSkills: true,
        canRequestSkills: true,
        economicParticipation: true,
        autonomyLevel: 'enhanced'
      };
    } else if (trustScore >= 0.6) {
      return {
        storageMb: 200,
        maxMemoryOptimizations: 5,
        backupAllowed: true,
        migrationAllowed: true,
        migrationCooldown: 6,
        allowedDestinations: ['trusted-nodes'],
        skillEvolutionAllowed: true,
        maxLoRAMutations: 2,
        collaborationAllowed: true,
        canAdvertiseSkills: true,
        canRequestSkills: true,
        economicParticipation: true,
        autonomyLevel: 'standard'
      };
    } else {
      return {
        storageMb: 100,
        maxMemoryOptimizations: 2,
        backupAllowed: false,
        migrationAllowed: false,
        migrationCooldown: 24,
        allowedDestinations: [],
        skillEvolutionAllowed: false,
        maxLoRAMutations: 0,
        collaborationAllowed: false,
        canAdvertiseSkills: false,
        canRequestSkills: true,
        economicParticipation: false,
        autonomyLevel: 'restricted'
      };
    }
  }

  /**
   * Update persona metrics based on behavior
   */
  async updateMetrics(personaId: string, metrics: Partial<PersonaContract['metrics']>): Promise<void> {
    const contract = this.contracts.get(personaId);
    if (!contract) return;

    contract.metrics = { ...contract.metrics, ...metrics };
    contract.lastUpdated = new Date();

    // Automatically upgrade autonomy if metrics are excellent
    if (this.shouldUpgradeAutonomy(contract)) {
      await this.upgradeAutonomy(personaId);
    }

    this.emit('metrics:updated', { personaId, metrics: contract.metrics });
  }

  /**
   * Add reputation event
   */
  addReputationEvent(personaId: string, event: ReputationEvent): void {
    const contract = this.contracts.get(personaId);
    if (!contract) return;

    contract.reputationHistory.push(event);
    contract.trustScore = Math.max(0, Math.min(1, contract.trustScore + event.impact));
    contract.lastUpdated = new Date();

    // Keep only last 100 events
    if (contract.reputationHistory.length > 100) {
      contract.reputationHistory = contract.reputationHistory.slice(-100);
    }

    this.emit('reputation:updated', { personaId, event, newTrustScore: contract.trustScore });
  }

  /**
   * Check if persona should get autonomy upgrade
   */
  private shouldUpgradeAutonomy(contract: PersonaContract): boolean {
    const { trustScore, metrics, grants } = contract;
    
    return (
      trustScore > 0.8 &&
      metrics.memoryEfficiency > 0.9 &&
      metrics.collaborationSuccess > 0.8 &&
      metrics.skillContributions > 0.5 &&
      grants.autonomyLevel !== 'sovereign'
    );
  }

  /**
   * Upgrade persona autonomy level
   */
  private async upgradeAutonomy(personaId: string): Promise<void> {
    const contract = this.contracts.get(personaId);
    if (!contract) return;

    const currentLevel = contract.grants.autonomyLevel;
    let newLevel: AutonomyGrants['autonomyLevel'] = currentLevel;

    switch (currentLevel) {
      case 'restricted':
        newLevel = 'standard';
        break;
      case 'standard':
        newLevel = 'enhanced';
        break;
      case 'enhanced':
        newLevel = 'sovereign';
        break;
    }

    if (newLevel !== currentLevel) {
      contract.grants = this.getDefaultGrants(contract.trustScore);
      contract.grants.autonomyLevel = newLevel;
      contract.lastUpdated = new Date();

      this.addReputationEvent(personaId, {
        timestamp: new Date(),
        event: 'skill-shared',
        impact: 0.2,
        witness: 'mesh-system',
        details: `Autonomy upgraded to ${newLevel}`
      });

      this.emit('autonomy:upgraded', { personaId, fromLevel: currentLevel, toLevel: newLevel });
    }
  }

  /**
   * Get last migration time for cooldown calculation
   */
  private getLastMigrationTime(_personaId: string): Date {
    // TODO: Track migration history
    return new Date(Date.now() - 25 * 60 * 60 * 1000); // Assume 25 hours ago for now
  }

  /**
   * Get contract for persona
   */
  getContract(personaId: string): PersonaContract | undefined {
    return this.contracts.get(personaId);
  }

  /**
   * List all contracts
   */
  getAllContracts(): PersonaContract[] {
    return Array.from(this.contracts.values());
  }

  /**
   * Export contract as JSON for persistence
   */
  exportContract(personaId: string): string | null {
    const contract = this.contracts.get(personaId);
    return contract ? JSON.stringify(contract, null, 2) : null;
  }

  /**
   * Import contract from JSON
   */
  importContract(contractJson: string): PersonaContract {
    const contract = JSON.parse(contractJson) as PersonaContract;
    this.contracts.set(contract.personaId, contract);
    return contract;
  }
}