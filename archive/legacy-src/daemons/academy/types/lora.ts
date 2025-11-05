/**
 * LoRA Layer and Composition Types
 */

export interface LoRALayer {
  layer_id: string;
  source_id?: string;  // For CapabilitySynthesis compatibility
  layer_type: string;
  name?: string;       // For LayerOptimization compatibility
  domain: string;
  rank: number;
  alpha: number;
  target_modules: string[];
  weight?: number;     // For CapabilitySynthesis composition
  position?: 'core' | 'bridge' | 'novel'; // For CapabilitySynthesis
  performance_metrics?: any; // For LayerOptimization
}

export interface NovelLayer {
  target_domain: string;
  required_capabilities: string[];
  creation_strategy: string;
  estimated_rank: number;
  confidence: number;
}

export interface LoRAComposition {
  primary_layers: LoRALayer[];          // Core capability layers
  bridge_layers: LoRALayer[];           // Layers that connect different domains
  novel_layers: NovelLayer[];           // New layers we need to create
  composition_algorithm: string;         // How to stack/blend them
  total_rank: number;                   // Combined complexity
  compression_efficiency: number;       // Storage efficiency of composition
}

export interface LoRAStack {
  layers: LoRALayer[];
  composition_algorithm: string;
  total_parameters: number;
  compression_ratio: number;
  activation_patterns: ActivationPattern[];
}

export interface ActivationPattern {
  pattern_name: string;
  activation_sequence: number[];
  frequency: number;
  context_triggers: string[];
}