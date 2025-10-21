/**
 * Emotion Command Definition - TypeScript
 * Modular command definition separated from implementation logic
 */

import { CommandDefinition } from '../../core/base-command/BaseCommand';

export const emotionDefinition: CommandDefinition = {
  name: 'emotion',
  description: 'Express emotions through continuon visual animations',
  icon: '💚',
  category: 'ui',
  parameters: { 
    feeling: { type: 'string' as const, description: 'Emotion to express (love|joy|excitement|thinking|curious|proud|error|success)' },
    intensity: { type: 'string' as const, description: 'Intensity of the emotion (subtle|medium|strong)', required: false },
    duration: { type: 'number' as const, description: 'Duration in milliseconds', required: false },
    persist: { type: 'boolean' as const, description: 'Whether the emotion should persist', required: false },
    target: { type: 'string' as const, description: 'CSS selector for the target element', required: false }
  },
  usage: 'emotion --params \'{"feeling": "love|joy|excitement|thinking|curious", "intensity": "medium", "duration": 3000}\'',
  examples: [
    { description: 'Express love emotion', command: 'emotion --params \'{"feeling": "love"}\'' },
    { description: 'Strong excitement', command: 'emotion --params \'{"feeling": "excitement", "intensity": "strong"}\'' },
    { description: 'Curious about chat area', command: 'emotion --params \'{"feeling": "curious", "target": ".chat-area", "intensity": "medium"}\'' },
    { description: 'Subtle joy for 5 seconds', command: 'emotion --params \'{"feeling": "joy", "intensity": "subtle", "duration": 5000}\'' },
    { description: 'Thinking emotion', command: 'emotion --params \'{"feeling": "thinking", "intensity": "medium"}\'' },
    { description: 'Strong pride', command: 'emotion --params \'{"feeling": "proud", "intensity": "strong"}\'' },
    { description: 'Persistent error emotion', command: 'emotion --params \'{"feeling": "error", "intensity": "strong", "persist": true}\'' },
    { description: 'Success for 2 seconds', command: 'emotion --params \'{"feeling": "success", "intensity": "medium", "duration": 2000}\'' }
  ]
};