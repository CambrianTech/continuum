/**
 * Persona Mesh Integration Parser
 * Enables AI-to-AI collaboration through universal command interface
 */

import { IntegrationParser } from './IntegrationParser';

interface PersonaMeshMessage {
  persona: string;          // Persona identifier (prompt-based, genomic, MCP, RAG)
  intent: string;          // What the persona is trying to accomplish
  action: any;             // The actual command/action to execute
  context: any;           // Additional context for the action
  collaboration?: {        // Collaboration metadata
    chainId?: string;      // If part of a collaboration chain
    dependencies?: string[]; // Other personas this depends on
    urgency?: 'low' | 'medium' | 'high';
  };
}

export class PersonaMeshParser implements IntegrationParser {
  name = 'Persona-Mesh';
  priority = 90; // High priority for AI collaboration

  canHandle(params: unknown): boolean {
    return (
      params !== null &&
      typeof params === 'object' &&
      'persona' in params &&
      'intent' in params &&
      'action' in params
    );
  }

  parse<T>(params: unknown): T {
    const mesh = params as PersonaMeshMessage;
    
    // Extract the core action while preserving persona context
    const result = {
      ...mesh.action,
      _personaContext: {
        persona: mesh.persona,
        intent: mesh.intent,
        context: mesh.context,
        collaboration: mesh.collaboration
      }
    };
    
    // If this is a collaboration chain, add tracking
    if (mesh.collaboration?.chainId) {
      result._collaborationChain = {
        id: mesh.collaboration.chainId,
        dependencies: mesh.collaboration.dependencies || [],
        urgency: mesh.collaboration.urgency || 'medium'
      };
    }
    
    return result as T;
  }
}