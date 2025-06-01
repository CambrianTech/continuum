export interface Task {
  prompt: string
  tokens: number
  type?: 'file_search' | 'complex_inference' | 'lookup' | 'wakeup'
  urgency?: 'low' | 'medium' | 'high'
  isSensitive?: boolean
  priorMemory?: boolean
}

export interface Agent {
  name: string
  run: (task: Task) => Promise<string>
  costEstimate: (tokens: number) => number
  capabilities: string[]
}
