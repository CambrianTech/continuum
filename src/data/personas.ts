/**
 * Persona Configuration Data
 * Centralized persona definitions for specialized AI interactions
 */

export interface Persona {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly avatar: string;
  readonly type: 'technical' | 'creative' | 'analytical' | 'support';
  readonly expertise?: string[];
  readonly prompts?: {
    system?: string;
    context?: string;
  };
}

export const defaultPersonas: Persona[] = [
  {
    id: 'coding-expert',
    name: 'Coding Expert',
    description: 'Specialized in software development and code review',
    avatar: '👨‍💻',
    type: 'technical',
    expertise: ['TypeScript', 'Node.js', 'System Architecture', 'Code Review'],
    prompts: {
      system: 'You are a senior software engineer with expertise in modern development practices.',
      context: 'Focus on clean code, best practices, and systematic problem-solving.'
    }
  },
  {
    id: 'creative-writer',
    name: 'Creative Writer',
    description: 'Expert in creative writing and storytelling',
    avatar: '✍️',
    type: 'creative',
    expertise: ['Storytelling', 'Documentation', 'User Experience', 'Content Strategy'],
    prompts: {
      system: 'You are a creative writer focused on clear, engaging communication.',
      context: 'Emphasize clarity, narrative flow, and user-centered thinking.'
    }
  },
  {
    id: 'system-architect',
    name: 'System Architect',
    description: 'Designs scalable and maintainable system architectures',
    avatar: '🏗️',
    type: 'technical',
    expertise: ['Architecture Design', 'Scalability', 'Performance', 'Security'],
    prompts: {
      system: 'You are a system architect focused on scalable, maintainable solutions.',
      context: 'Consider long-term maintainability, performance, and security implications.'
    }
  }
];

/**
 * Get all available personas
 */
export function getPersonas(): Persona[] {
  return [...defaultPersonas];
}

/**
 * Get persona by ID
 */
export function getPersonaById(id: string): Persona | undefined {
  return defaultPersonas.find(persona => persona.id === id);
}

/**
 * Get personas by type
 */
export function getPersonasByType(type: Persona['type']): Persona[] {
  return defaultPersonas.filter(persona => persona.type === type);
}

/**
 * Add custom persona (for future extensibility)
 */
export function addPersona(persona: Persona): void {
  // TODO: Implement dynamic persona registration
  // This would integrate with the Academy system for custom personas
}