import { routeTask } from './router'

async function main() {
  const result = await routeTask({
    prompt: 'Refactor this function to be more testable',
    tokens: 800,
    type: 'complex_inference',
    urgency: 'medium',
    priorMemory: true,
  })

  console.log(`🤖 ${result.agent.name} replied:`)
  console.log(result.response)
}

main().catch((err) => {
  console.error('Error running task:', err)
  process.exit(1)
})
