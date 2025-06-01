import { Agent, Task } from '../types'
import { sendPrompt, getLatestResponse } from '../../llama-bootstrap/llama-runner'

export const LlamaAgent: Agent = {
  name: 'llama-local',
  costEstimate: (_) => 0,
  capabilities: ['lookup', 'file_search', 'indexing'],

  async run(task: Task): Promise<string> {
    sendPrompt(task.prompt)
    await new Promise((r) => setTimeout(r, 1500)) // crude delay to wait for output
    return getLatestResponse()
  }
}
