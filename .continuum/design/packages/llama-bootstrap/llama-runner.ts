import { execSync } from 'child_process'
import fs from 'fs'
import path from 'path'

const session = 'llama'
const logFile = path.resolve(__dirname, 'llama-output.log')

// Send a prompt to LLaMA via tmux
export function sendPrompt(prompt: string) {
  const safe = prompt.replace(/"/g, '\\"')
  execSync(`tmux send-keys -t ${session} "${safe}" C-m`)
}

// Read the last few lines from the output log
export function getLatestResponse(): string {
  if (!fs.existsSync(logFile)) return ''
  const lines = fs.readFileSync(logFile, 'utf-8').split('\n')
  return lines.slice(-10).join('\n')
}
