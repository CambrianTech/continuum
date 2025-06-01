
// install-model.ts - Install model plugins or link APIs

import fs from 'fs';
import path from 'path';

const AGENT_REGISTRY = path.join(".continuum", "agents.json");

function installModel(id: string) {
  const registry = require("./registry.json");
  const found = registry.models.find(m => m.id === id);
  if (!found) {
    console.error("❌ Model not found.");
    return;
  }

  if (found.provider === "local") {
    console.log(`🔧 Installing local model: ${found.name}`);
    console.log(`Run: ${found.install}`);
  } else {
    console.log(`🔑 Link API for ${found.name}:`);
    console.log(`Run: ${found.install}`);
  }

  // Append to agent config
  if (!fs.existsSync(AGENT_REGISTRY)) {
    fs.writeFileSync(AGENT_REGISTRY, JSON.stringify({ agents: [] }, null, 2));
  }

  const current = JSON.parse(fs.readFileSync(AGENT_REGISTRY, "utf8"));
  current.agents.push({ id: found.id, name: found.name, capabilities: found.capabilities });
  fs.writeFileSync(AGENT_REGISTRY, JSON.stringify(current, null, 2));

  console.log("✅ Model registered.");
}

const modelId = process.argv[2];
if (!modelId) {
  console.log("Usage: npx continuum install-model <model-id>");
} else {
  installModel(modelId);
}
