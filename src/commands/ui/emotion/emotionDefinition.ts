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
  params: '{"feeling": "string", "intensity": "subtle|medium|strong|overwhelming", "duration": "number", "persist": "boolean", "target": "string"}',
  usage: 'emotion --params \'{"feeling": "love|joy|excitement|thinking|curious", "intensity": "medium", "duration": 3000}\'',
  examples: [
    'emotion --params \'{"feeling": "love"}\'',
    'emotion --params \'{"feeling": "excitement", "intensity": "strong"}\'',
    'emotion --params \'{"feeling": "curious", "target": ".chat-area", "intensity": "medium"}\'',
    'emotion --params \'{"feeling": "joy", "intensity": "subtle", "duration": 5000}\'',
    'emotion --params \'{"feeling": "thinking", "intensity": "medium"}\'',
    'emotion --params \'{"feeling": "proud", "intensity": "strong"}\'',
    'emotion --params \'{"feeling": "error", "intensity": "strong", "persist": true}\'',
    'emotion --params \'{"feeling": "success", "intensity": "medium", "duration": 2000}\''
  ]
};