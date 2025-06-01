import { Agent, Task } from '../types'

export const ClaudeAgent: Agent = {
  name: 'claude',
  costEstimate: (tokens) => tokens * 0.00002, // example cost/token

  capabilities: ['deep_inference', 'wakeup', 'complex_code'],

  async run(task: Task): Promise<string> {
    // Insert Claude Code / Claude API call here
    return `[Claude response to task: ${task.prompt}]`
  },
}
