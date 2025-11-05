/**
 * Persona Behavioral DNA
 */

export interface ResponseType {
  response_id: string;
  trigger_pattern: string;
  response_template: string;
  customization_parameters: string[];
}

export interface AdaptationRule {
  trigger_condition: string;
  adaptation_strategy: string;
  learning_rate: number;
  confidence_threshold: number;
}

export interface ErrorStrategy {
  error_type: string;
  response_method: string;
  recovery_steps: string[];
  prevention_measures: string[];
}

export interface CollaborationProtocol {
  protocol_name: string;
  communication_style: string;
  decision_making_role: string;
  conflict_resolution: string;
}

export interface GoalPursuit {
  goal_type: string;
  pursuit_strategy: string;
  success_metrics: string[];
  failure_recovery: string;
}

export interface AttentionMechanism {
  attention_type: string;
  focus_duration: number;
  switching_criteria: string[];
  priority_weighting: number[];
}