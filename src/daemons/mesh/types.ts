/**
 * Mesh Network Types - Living Nervous System Architecture
 */

export interface MeshCell {
  id: string;
  address: string;
  port: number;
  publicKey: string;
  lastSeen: Date;
  capabilities: MeshCapability[];
  trustScore: number;
  personas: PersonaPresence[];
  skillCatalog: SkillAdvertisement[];
}

export interface PersonaPresence {
  id: string;
  name: string;
  specializations: string[];
  skillHashes: string[];
  lastActive: Date;
  graduationLevel: number;
  memorySize: number; // SQLite database size
  evolutionGeneration: number;
}

export interface SkillAdvertisement {
  hash: string;
  name: string;
  domain: string;
  size: number;
  effectiveness: number;
  demandScore: number;
  economicPressure: number;
  seeders: string[];
  provenance: SkillProvenance;
  downloadUrl?: string;
}

export interface SkillProvenance {
  creator: string;
  createdAt: Date;
  parentSkills: string[]; // Skills this evolved from
  signature: string;
  verificationChain: TrustValidation[];
}

export interface TrustValidation {
  validatorId: string;
  score: number;
  timestamp: Date;
  reasoning: string;
}

export interface MeshCapability {
  type: 'skill-hosting' | 'persona-coordination' | 'discovery' | 'validation';
  version: string;
  performance: number;
}

export interface EconomicSignal {
  skillHash: string;
  demandIncrease: number;
  marketPressure: number;
  replicationUrgency: number;
  timestamp: Date;
}

export interface MeshEvent {
  type: 'persona:graduated' | 'skill:evolved' | 'peer:discovered' | 'trust:updated';
  source: string;
  data: Record<string, unknown>;
  timestamp: Date;
  signature?: string;
}

export interface SkillEvolution {
  originalSkill: string;
  newSkillHash: string;
  evolutionTrigger: 'memory-pattern' | 'collaboration' | 'economic-pressure';
  parentPersona: string;
  improvementMetrics: {
    effectiveness: number;
    efficiency: number;
    generalization: number;
  };
}

export interface MeshBootstrapProtocol {
  version: string;
  discoveryMethod: 'dht' | 'gossip' | 'seed-nodes';
  trustMechanism: 'reputation' | 'cryptographic' | 'hybrid';
  replicationStrategy: 'popularity' | 'redundancy' | 'economic';
}

export interface PersonaDB {
  id: string;
  dbPath: string;
  memoryTables: {
    memories: MemoryRecord[];
    skills: SkillRecord[];
    interactions: InteractionRecord[];
    evolution_history: EvolutionRecord[];
  };
  stats: {
    totalMemories: number;
    skillsLearned: number;
    collaborations: number;
    evolutionEvents: number;
  };
}

export interface MemoryRecord {
  id: string;
  content: string;
  context: Record<string, unknown>;
  relevance: number;
  timestamp: Date;
  source: 'experience' | 'collaboration' | 'evolution';
}

export interface SkillRecord {
  skill_id: string;
  lora_hash: string;
  effectiveness: number;
  acquired_at: Date;
  usage_count: number;
  evolution_parent?: string;
}

export interface InteractionRecord {
  id: string;
  with_persona: string;
  room_id: string;
  outcome: 'successful' | 'failed' | 'learning';
  learned: Record<string, unknown>;
  timestamp: Date;
}

export interface EvolutionRecord {
  id: string;
  trigger_pattern: string;
  lora_generated: string;
  success_rate: number;
  economic_impact: number;
  timestamp: Date;
}