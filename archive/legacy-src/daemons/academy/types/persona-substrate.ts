/**
 * Persona Substrate and Technical Implementation
 */

export interface ModelArchitecture {
  base_model: string;
  model_size: string;
  architecture_type: string;
  layer_count: number;
  parameter_count: number;
}

export interface ComputeProfile {
  cpu_cores: number;
  memory_gb: number;
  gpu_count: number;
  storage_gb: number;
  bandwidth_mbps: number;
}

export interface MemoryStructure {
  structure_type: string;
  capacity: number;
  access_pattern: string;
  retention_policy: string;
}

export interface ExecutionEnv {
  environment_type: string;
  runtime_version: string;
  dependencies: string[];
  security_constraints: string[];
}

export interface InterfaceProtocol {
  protocol_name: string;
  version: string;
  data_format: string;
  authentication_method: string;
}