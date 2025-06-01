import { execSync } from 'child_process'
import fs from 'fs'
import path from 'path'

const llamaDir = path.resolve(__dirname, 'bin')

export function installLlama() {
  console.log('🔧 Installing llama.cpp...')

  if (!fs.existsSync(llamaDir)) fs.mkdirSync(llamaDir)

  execSync(`
    git clone https://github.com/ggerganov/llama.cpp ${llamaDir}/llama.cpp &&
    cd ${llamaDir}/llama.cpp &&
    make
  `, { stdio: 'inherit', shell: true })

  console.log('✅ LLaMA compiled.')
}
