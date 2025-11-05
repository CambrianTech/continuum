/**
 * Persona Identity and Core Characteristics
 */

export interface CommunicationStyle {
  formality_level: number;
  verbosity_preference: number;
  explanation_depth: number;
  humor_usage: number;
  technical_terminology: number;
}

export interface DecisionFramework {
  risk_tolerance: number;
  speed_vs_accuracy: number;
  collaboration_preference: number;
  evidence_requirements: number;
  uncertainty_handling: string;
}

export interface ValueSystem {
  core_values: string[];
  ethical_constraints: EthicalConstraint[];
  priority_hierarchy: Priority[];
  conflict_resolution: ConflictResolution[];
}

export interface EthicalConstraint {
  constraint_type: string;
  description: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  enforcement_method: string;
}

export interface Priority {
  priority_name: string;
  weight: number;
  context_conditions: string[];
}

export interface ConflictResolution {
  conflict_type: string;
  resolution_strategy: string;
  success_rate: number;
}

export interface ThinkingPattern {
  name: string;
  description: string;
  trigger_conditions: string[];
  process_steps: ProcessStep[];
  output_format: string;
  success_metrics: string[];
}

export interface ProcessStep {
  step_name: string;
  step_description: string;
  input_requirements: string[];
  output_expectations: string[];
}