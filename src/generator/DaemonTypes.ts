/**
 * DaemonTypes - Type definitions for daemon specifications
 *
 * Used by DaemonGenerator to create daemon files from declarative specs
 */

export interface DaemonParam {
  type: 'string' | 'number' | 'boolean' | 'enum';
  default?: string | number | boolean;
  values?: string[]; // For enum type
  description: string;
}

export interface DaemonJob {
  name: string;
  params: Array<{
    name: string;
    type: string;
  }>;
  returns: string;
  async: boolean;
  description?: string;
}

export interface DaemonEvent {
  name: string;
  payload: Record<string, string>; // field name → type
  description?: string;
}

export interface DaemonLifecycle {
  onStart?: string;
  onStop?: string;
}

/**
 * Complete specification for generating a daemon
 */
export interface DaemonSpec {
  name: string; // kebab-case: 'lora-manager', 'training-pipeline'
  description: string;
  params?: Record<string, DaemonParam>;
  jobs: DaemonJob[];
  events?: DaemonEvent[];
  lifecycle?: DaemonLifecycle;
}
