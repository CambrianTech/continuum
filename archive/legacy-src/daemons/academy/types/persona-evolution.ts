/**
 * Persona Evolution and Training
 */

export interface TrainingSession {
  session_id: string;
  training_type: 'adversarial' | 'collaborative' | 'self_directed';
  challenges_faced: Challenge[];
  adaptations_made: Adaptation[];
  performance_delta: PerformanceDelta;
  emergent_behaviors: string[];
  trainer_notes: string[];
}

export interface Challenge {
  challenge_id: string;
  challenge_type: string;
  difficulty_level: number;
  domain: string;
  resolution_approach: string;
}

export interface Adaptation {
  adaptation_id: string;
  adaptation_type: string;
  trigger_condition: string;
  modification_description: string;
  success_rate: number;
}

export interface PerformanceDelta {
  before_metrics: Record<string, number>;
  after_metrics: Record<string, number>;
  improvement_areas: string[];
  regression_areas: string[];
}

export interface PersonaAncestry {
  parent_personas: string[];
  genetic_contributions: GeneticContribution[];
  crossover_points: CrossoverPoint[];
  inherited_traits: InheritedTrait[];
  novel_mutations: NovelMutation[];
}

export interface GeneticContribution {
  parent_id: string;
  contribution_percentage: number;
  inherited_components: string[];
  novel_variations: string[];
}

export interface CrossoverPoint {
  crossover_location: string;
  parent_a_contribution: string[];
  parent_b_contribution: string[];
  crossover_strategy: string;
}

export interface InheritedTrait {
  trait_name: string;
  source_parent: string;
  expression_strength: number;
  modification_level: number;
}

export interface NovelMutation {
  mutation_id: string;
  mutation_description: string;
  emergence_trigger: string;
  stability_assessment: number;
}

export interface Mutation {
  mutation_id: string;
  mutation_type: string;
  target_component: string;
  change_description: string;
  impact_score: number;
}

export interface SelectionPressure {
  pressure_source: string;
  intensity: number;
  target_traits: string[];
  adaptation_direction: string;
}

export interface EmergentProperty {
  property_name: string;
  emergence_conditions: string[];
  manifestation_examples: string[];
  stability_score: number;
}

export interface FitnessMetrics {
  overall_fitness: number;
  domain_specific_scores: Record<string, number>;
  adaptation_rate: number;
  survival_probability: number;
}

export interface MutationPotential {
  mutation_rate: number;
  allowed_mutations: string[];
  stability_constraints: string[];
}