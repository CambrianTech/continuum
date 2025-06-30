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
  parameters: { feeling: 'string', intensity: 'string', duration: 'number', persist: 'boolean', target: 'string' },
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