
// link-agent.ts - Save user API keys to .env safely

import fs from 'fs';
import path from 'path';

const envPath = path.join(process.cwd(), '.env');

function storeKey(provider: string, key: string) {
  let env = '';
  if (fs.existsSync(envPath)) {
    env = fs.readFileSync(envPath, 'utf8');
  }

  const varName = provider.toUpperCase() + '_API_KEY';
  const updated = env.replace(new RegExp(`${varName}=.*`, 'g'), '');
  const finalEnv = updated + `\n${varName}=${key}\n`;
  fs.writeFileSync(envPath, finalEnv);
  console.log(`🔐 Saved ${provider} key to .env`);
}

const [provider, key] = process.argv.slice(2);
if (!provider || !key) {
  console.log("Usage: npx continuum link-agent <provider> <key>");
} else {
  storeKey(provider, key);
}
