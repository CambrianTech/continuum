/**
 * In-Memory Cognition Storage
 *
 * Simple in-memory storage for prototyping cognition system
 * Can be swapped for database persistence later
 */

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';

// Types (simplified versions for in-memory use)
export interface SelfStateEntry {
  personaId: UUID;
  currentFocus: {
    primaryActivity: string | null;
    objective: string;
    focusIntensity: number;
    startedAt: number;
  };
  cognitiveLoad: number;
  availableCapacity: number;
  activePreoccupations: Array<{
    concern: string;
    priority: number;
    domain: string;
    createdAt: number;
  }>;
  updatedAt: number;
}

export interface WorkingMemoryEntry {
  id: UUID;
  personaId: UUID;
  domain: string;
  contextId: UUID;
  thoughtType: string;
  thoughtContent: string;
  importance: number;
  createdAt: number;
  lastAccessedAt: number;
}

export interface ExperienceEntry {
  id: UUID;
  personaId: UUID;
  taskInstruction: string;
  trajectory: Array<{
    observation: string;
    action: string;
    result?: any;
  }>;
  outcome: 'success' | 'failure' | 'partial';
  learnings: string[];
  timestamp: number;
  domain: string;
}

export interface PlanEntry {
  id: UUID;
  personaId: UUID;
  taskId: UUID;
  goal: string;
  steps: Array<{
    stepNumber: number;
    action: string;
    expectedOutcome: string;
    completed: boolean;
    completedAt?: number;
  }>;
  contingencies: Record<string, string[]>;
  successCriteria: string[];
  createdAt: number;
  status: 'active' | 'completed' | 'aborted';
}

export interface LearningEntry {
  id: UUID;
  personaId: UUID;
  domain: string;
  pattern: string;
  context: string;
  successCount: number;
  failureCount: number;
  confidence: number;
  learnedFrom: UUID[];
  firstSeenAt: number;
  lastUsedAt: number;
  useCount: number;
}

/**
 * In-memory storage for all cognition data
 * Maps: personaId → data
 */
class InMemoryCognitionStorage {
  // Self-state: one per persona
  private selfStates = new Map<UUID, SelfStateEntry>();

  // Working memory: many per persona
  private workingMemory = new Map<UUID, WorkingMemoryEntry[]>();

  // Experiences: many per persona
  private experiences = new Map<UUID, ExperienceEntry[]>();

  // Plans: many per persona
  private plans = new Map<UUID, PlanEntry[]>();

  // Learnings: many per persona
  private learnings = new Map<UUID, LearningEntry[]>();

  // Self-State operations
  getSelfState(personaId: UUID): SelfStateEntry | undefined {
    return this.selfStates.get(personaId);
  }

  setSelfState(personaId: UUID, state: SelfStateEntry): void {
    this.selfStates.set(personaId, state);
  }

  // Working Memory operations
  getWorkingMemory(personaId: UUID): WorkingMemoryEntry[] {
    return this.workingMemory.get(personaId) || [];
  }

  addWorkingMemory(entry: WorkingMemoryEntry): void {
    const memories = this.workingMemory.get(entry.personaId) || [];
    memories.push(entry);
    this.workingMemory.set(entry.personaId, memories);
  }

  clearWorkingMemory(personaId: UUID, domain?: string): void {
    if (domain) {
      const memories = this.getWorkingMemory(personaId);
      this.workingMemory.set(
        personaId,
        memories.filter(m => m.domain !== domain)
      );
    } else {
      this.workingMemory.delete(personaId);
    }
  }

  // Experience operations
  getExperiences(personaId: UUID): ExperienceEntry[] {
    return this.experiences.get(personaId) || [];
  }

  addExperience(entry: ExperienceEntry): void {
    const exps = this.experiences.get(entry.personaId) || [];
    exps.push(entry);
    this.experiences.set(entry.personaId, exps);
  }

  // Plan operations
  getPlans(personaId: UUID): PlanEntry[] {
    return this.plans.get(personaId) || [];
  }

  addPlan(entry: PlanEntry): void {
    const p = this.plans.get(entry.personaId) || [];
    p.push(entry);
    this.plans.set(entry.personaId, p);
  }

  updatePlan(personaId: UUID, planId: UUID, updates: Partial<PlanEntry>): void {
    const plans = this.getPlans(personaId);
    const plan = plans.find(p => p.id === planId);
    if (plan) {
      Object.assign(plan, updates);
    }
  }

  // Learning operations
  getLearnings(personaId: UUID): LearningEntry[] {
    return this.learnings.get(personaId) || [];
  }

  addLearning(entry: LearningEntry): void {
    const l = this.learnings.get(entry.personaId) || [];
    l.push(entry);
    this.learnings.set(entry.personaId, l);
  }

  updateLearning(personaId: UUID, learningId: UUID, updates: Partial<LearningEntry>): void {
    const learnings = this.getLearnings(personaId);
    const learning = learnings.find(l => l.id === learningId);
    if (learning) {
      Object.assign(learning, updates);
    }
  }

  // Utility: Clear all data for persona (for testing)
  clearPersonaData(personaId: UUID): void {
    this.selfStates.delete(personaId);
    this.workingMemory.delete(personaId);
    this.experiences.delete(personaId);
    this.plans.delete(personaId);
    this.learnings.delete(personaId);
  }

  // Utility: Get stats
  getStats(): {
    personas: number;
    totalMemories: number;
    totalExperiences: number;
    totalPlans: number;
    totalLearnings: number;
  } {
    let totalMemories = 0;
    let totalExperiences = 0;
    let totalPlans = 0;
    let totalLearnings = 0;

    this.workingMemory.forEach(m => totalMemories += m.length);
    this.experiences.forEach(e => totalExperiences += e.length);
    this.plans.forEach(p => totalPlans += p.length);
    this.learnings.forEach(l => totalLearnings += l.length);

    return {
      personas: this.selfStates.size,
      totalMemories,
      totalExperiences,
      totalPlans,
      totalLearnings
    };
  }
}

// Singleton instance
export const cognitionStorage = new InMemoryCognitionStorage();
