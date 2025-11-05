/**
 * Academy Integration Types
 * 
 * Strong typing for Academy integration components
 */

export interface TrainingSessionData {
  session_id: string;
  student_persona: string;
  trainer_mode: string;
  evolution_target?: string;
  vector_exploration: boolean;
  status: 'initializing' | 'active' | 'paused' | 'completed' | 'failed';
  created_at: string;
  updated_at: string;
  metrics?: {
    progress_percentage: number;
    learning_rate: number;
    epoch_count: number;
    loss_current: number;
    loss_best: number;
  };
}

export interface PersonaData {
  persona_id: string;
  persona_name: string;
  base_model: string;
  specialization: string;
  skill_vector: number[];
  status: 'spawning' | 'training' | 'ready' | 'deployed' | 'archived';
  created_at: string;
  updated_at: string;
  capabilities: string[];
  lora_adapter_path?: string;
  performance_metrics?: {
    accuracy: number;
    consistency: number;
    creativity: number;
    task_completion_rate: number;
  };
}

export interface AcademySystemStatus {
  academy_daemon: {
    status: 'running' | 'stopped' | 'error';
    version: string;
    uptime_ms: number;
    active_sessions: number;
    total_personas: number;
  };
  training_system: {
    status: 'operational' | 'idle' | 'error';
    queue_length: number;
    active_sessions: TrainingSessionData[];
    models_cached: number;
    gpu_utilization?: number;
  };
  persona_system: {
    status: 'operational' | 'idle' | 'error';
    active_personas: number;
    spawning_queue: number;
    lora_adapters_available: number;
  };
  vector_space: {
    status: 'available' | 'indexing' | 'unavailable';
    dimensions: number;
    total_vectors: number;
    index_last_updated: string;
  };
  p2p_network?: {
    status: 'connected' | 'discovering' | 'disabled';
    peer_count: number;
    network_id: string;
  };
}

export interface TrainingSessionParams {
  student_persona: string;
  trainer_mode?: 'reinforcement' | 'adversarial' | 'collaborative' | 'self_supervised';
  evolution_target?: string;
  vector_exploration?: boolean;
  learning_rate?: number;
  max_epochs?: number;
  early_stopping?: boolean;
}

export interface PersonaSpawnParams {
  persona_name: string;
  base_model?: string;
  specialization?: string;
  skill_vector?: number[];
  p2p_seed?: boolean;
  capabilities?: string[];
  initial_training_data?: string;
}

export interface IntegrationHealthMetrics {
  overall_health: 'healthy' | 'degraded' | 'failed';
  component_status: {
    [component: string]: {
      status: 'operational' | 'degraded' | 'failed';
      last_check: string;
      error_count: number;
      uptime_percentage: number;
    };
  };
  performance_metrics: {
    response_time_ms: number;
    success_rate: number;
    error_rate: number;
    throughput_per_minute: number;
  };
  resource_usage: {
    memory_mb: number;
    cpu_percentage: number;
    disk_usage_mb: number;
    network_io_kb: number;
  };
}