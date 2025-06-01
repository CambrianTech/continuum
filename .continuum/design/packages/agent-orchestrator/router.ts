import { Task, Agent } from './types'
import { loadAgents } from './loadAgents'

const AGENTS = loadAgents()

export async function routeTask(task: Task): Promise<{ agent: Agent, response: string }> {
  for (const agent of AGENTS) {
    if (agent.capabilities.includes(task.type || '')) {
      return { agent, response: await agent.run(task) }
    }
  }

  // Fallback to first enabled
  const fallback = AGENTS[0]
  return { agent: fallback, response: await fallback.run(task) }
}
