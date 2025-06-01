import fs from 'fs'
import path from 'path'
import { Agent } from './types'
import { ClaudeAgent } from './agents/claude'
import { LlamaAgent } from './agents/llama-local'

const manifestPath = path.resolve(__dirname, './agents.json')

export function loadAgents(): Agent[] {
  const config = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'))

  const agents: Agent[] = []

  if (config['claude']?.enabled) {
    agents.push(ClaudeAgent)
  }

  if (config['llama-local']?.enabled) {
    agents.push(LlamaAgent)
  }

  return agents.sort((a, b) => {
    const priA = config[a.name]?.priority || 0
    const priB = config[b.name]?.priority || 0
    return priB - priA
  })
}
