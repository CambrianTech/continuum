/**
 * Complete Persona Genome Definition
 * 
 * The full specification of what makes a persona - beyond simple metadata.
 * This is the "DNA" that enables complete persona reproduction, evolution,
 * and cross-system compatibility in the Academy mesh network.
 */

import { ValueSystem, ThinkingPattern, CommunicationStyle, DecisionFramework } from './persona-identity';
import { DomainExpertise, SkillGraph, ExperienceMemory, PatternLibrary, MentalModel, KnowledgeGap } from './persona-knowledge';
import { ResponseType, AdaptationRule, ErrorStrategy, CollaborationProtocol, GoalPursuit, AttentionMechanism } from './persona-behavior';
import { TrainingSession, PersonaAncestry, Mutation, SelectionPressure, EmergentProperty, FitnessMetrics } from './persona-evolution';
import { ModelArchitecture, ComputeProfile, MemoryStructure, ExecutionEnv, InterfaceProtocol } from './persona-substrate';
// TODO: LoRAStack interface needs to be defined in persona-substrate.ts
interface LoRAStack {
  layers: any[];
  configuration: Record<string, any>;
}

import { CreationAlgorithm, DependencyTree, InitStep, ValidationTest, BreedingProfile } from './persona-reproduction';
// TODO: MutationPotential interface needs to be defined in persona-reproduction.ts
interface MutationPotential {
  mutation_rate: number;
  potential_domains: string[];
}

/**
 * PersonaGenome - The complete specification for persona creation
 * 
 * This is the FULL GENOME of what makes a persona. Unlike simple metadata,
 * this captures everything needed to reproduce, evolve, and understand a persona:
 * 
 * - Complete behavioral specification (HOW it acts, not just what it does)
 * - Full knowledge graph with skill interconnections and confidence levels
 * - Evolutionary blueprint showing exact training history and mutations
 * - Reproduction recipe with step-by-step creation process
 * - Substrate mapping of the actual neural configuration
 * 
 * This enables "teleporting" personas across systems and true P2P compatibility.
 */
export interface PersonaGenome {
  // CORE IDENTITY - Who the persona is at its essence
  identity: {
    core_prompt: string;                      // Original Academy prompt that birthed this persona
    personality_matrix: number[][];           // Multi-dimensional personality encoding
    value_system: ValueSystem;               // What the persona cares about
    thinking_patterns: ThinkingPattern[];    // How it approaches problems
    communication_style: CommunicationStyle; // How it expresses itself
    decision_making_framework: DecisionFramework; // How it makes choices
  };

  // KNOWLEDGE ARCHITECTURE - What the persona knows and how
  knowledge: {
    domain_expertise: DomainExpertise[];     // What it knows deeply
    skill_graph: SkillGraph;                 // How skills interconnect
    experience_memories: ExperienceMemory[]; // Specific experiences that shaped it
    pattern_recognition: PatternLibrary;     // Patterns it has learned to recognize
    mental_models: MentalModel[];            // How it understands the world
    knowledge_gaps: KnowledgeGap[];          // What it knows it doesn't know
  };

  // BEHAVIORAL DNA - How the persona actually behaves
  behavior: {
    response_templates: ResponseType[];      // How it responds to situations
    adaptation_algorithms: AdaptationRule[];// How it learns and changes
    error_handling_strategies: ErrorStrategy[]; // How it deals with mistakes
    collaboration_protocols: CollaborationProtocol[]; // How it works with others
    goal_pursuit_methods: GoalPursuit[];     // How it achieves objectives
    attention_mechanisms: AttentionMechanism[]; // What it focuses on
  };

  // EVOLUTIONARY HISTORY - How the persona came to be
  evolution: {
    training_lineage: TrainingSession[];     // Every training session that shaped it
    genetic_ancestry: PersonaAncestry;       // What personas contributed to its creation
    mutation_history: Mutation[];           // How it has changed over time
    selection_pressures: SelectionPressure[]; // What environments shaped it
    emergent_properties: EmergentProperty[]; // Unexpected capabilities that emerged
    fitness_landscape: FitnessMetrics;       // Current performance across domains
  };

  // OPERATIONAL SUBSTRATE - The technical implementation
  substrate: {
    lora_stack: LoRAStack;                   // The actual neural adaptations
    model_architecture: ModelArchitecture;   // Base model and modifications
    compute_requirements: ComputeProfile;    // Resources needed to run
    memory_structures: MemoryStructure[];    // How it stores and retrieves information
    execution_environment: ExecutionEnv;     // Where and how it runs
    interface_protocols: InterfaceProtocol[]; // How it communicates with the world
  };

  // REPRODUCTION/CREATION INSTRUCTIONS - How to recreate this persona
  reproduction: {
    creation_algorithm: CreationAlgorithm;   // Exact steps to recreate this persona
    dependency_tree: DependencyTree;        // What other personas/components it needs
    initialization_sequence: InitStep[];     // Startup procedure
    validation_tests: ValidationTest[];      // How to verify it was created correctly
    breeding_compatibility: BreedingProfile; // What personas it can combine with
    mutation_potential: MutationPotential;   // How it can evolve further
  };
}

/**
 * Query interface for P2P persona discovery
 * 
 * When searching the Academy mesh, we're not just looking for metadata.
 * We're looking for personas with COMPATIBLE GENOMES that can work together,
 * be combined, or evolved for specific needs.
 */
export interface PersonaGenomeQuery {
  required_capabilities: CapabilityRequirement[];
  compatible_value_systems: ValueSystem[];
  minimum_domain_expertise: DomainRequirement[];
  behavioral_compatibility: BehavioralRequirement[];
  substrate_constraints: SubstrateConstraint[];
  
  // For P2P mesh queries
  genome_similarity_threshold: number;      // 0-1 how similar genomes need to be
  max_adaptation_distance: number;         // How much adaptation is acceptable
  breeding_compatibility_required: boolean; // Must be able to breed with existing personas
}

export interface CapabilityRequirement { 
  capability: string; 
  minimum_level: number; 
}

export interface DomainRequirement { 
  domain: string; 
  depth_required: number; 
}

export interface BehavioralRequirement { 
  pattern: string; 
  compatibility_level: number; 
}

export interface SubstrateConstraint { 
  requirement: string; 
  flexibility: number; 
}