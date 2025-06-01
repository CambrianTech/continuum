import { installLlama } from './install'
import fs from 'fs'
import path from 'path'

const modelsDir = path.resolve(__dirname, 'models')
const agentConfig = path.resolve(__dirname, '../agent-orchestrator/agents.json')

export async function init() {
  console.log('🚀 Starting LLaMA Bootstrap...')

  installLlama()

  // Download model if not present
  if (!fs.existsSync(modelsDir)) fs.mkdirSync(modelsDir)

  const modelPath = path.join(modelsDir, 'mistral-7b.Q4_K_M.gguf')
  if (!fs.existsSync(modelPath)) {
    console.log('📦 Downloading Mistral GGUF...')
    execSync(`curl -o ${modelPath} https://huggingface.co/...`, { stdio: 'inherit' }) // ← replace with valid link
  }

  // Update agents.json
  const agents = JSON.parse(fs.readFileSync(agentConfig, 'utf-8'))
  agents['llama-local'] = {
    enabled: true,
    priority: 1,
    capabilities: ['lookup', 'file_search', 'indexing'],
    binPath: './bin/llama.cpp/main',
    modelPath,
    maxTokens: 2048,
    costPerToken: 0
  }
  fs.writeFileSync(agentConfig, JSON.stringify(agents, null, 2))

  console.log('✅ LLaMA bootstrap complete.')
}
