/**
 * Emotion Command Definition
 * Modular command definition separated from implementation logic
 */

module.exports = {
  name: 'emotion',
  description: 'Express emotions through continuon visual animations',
  icon: '💚',
  category: 'continuon expression',
  parameters: {
    feeling: {
      type: 'string',
      required: true,
      description: 'Emotion to express: love, joy, sadness, excitement, thinking, surprised, sleepy, angry, curious, proud'
    },
    intensity: {
      type: 'string',
      required: false,
      default: 'medium',
      description: 'Emotion intensity: subtle, medium, strong, overwhelming'
    },
    duration: {
      type: 'number',
      required: false,
      default: 3000,
      description: 'Animation duration in milliseconds (0 for persistent states)'
    },
    persist: {
      type: 'boolean',
      required: false,
      default: false,
      description: 'Whether emotion should persist until cleared (for system states)'
    },
    target: {
      type: 'string',
      required: false,
      description: 'CSS selector to direct emotion towards (for love, curiosity, etc.)'
    }
  },
  examples: [
    {
      description: 'Express love with floating hearts',
      usage: 'emotion love'
    },
    {
      description: 'Show strong excitement with energy burst',
      usage: 'emotion excitement --intensity strong'
    },
    {
      description: 'Express curiosity towards a specific element',
      usage: 'emotion curious --target .chat-area --intensity medium'
    },
    {
      description: 'Show gentle joy for 5 seconds',
      usage: 'emotion joy --intensity subtle --duration 5000'
    },
    {
      description: 'Express thinking/processing state',
      usage: 'emotion thinking --intensity medium'
    },
    {
      description: 'Show pride with victory animation',
      usage: 'emotion proud --intensity strong'
    },
    {
      description: 'Indicate system error (persistent until cleared)',
      usage: 'emotion error --intensity strong'
    },
    {
      description: 'Show brief success celebration (fleeting)',
      usage: 'emotion success --intensity medium'
    },
    {
      description: 'Force fleeting love animation for 2 seconds',
      usage: 'emotion love --duration 2000 --persist false'
    }
  ]
};