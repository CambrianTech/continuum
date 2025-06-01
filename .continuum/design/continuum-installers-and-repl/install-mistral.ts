
// install-mistral.ts - Downloads Mistral model weights and config

import fs from 'fs';
import path from 'path';
import https from 'https';
import { execSync } from 'child_process';

const MODEL_URL = "https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.1-GGUF/resolve/main/mistral-7b-instruct-v0.1.Q4_K_M.gguf";
const modelDir = path.join(".continuum", "models");
const modelPath = path.join(modelDir, "mistral.gguf");

if (!fs.existsSync(modelDir)) fs.mkdirSync(modelDir, { recursive: true });

console.log("📦 Downloading Mistral model (may take a few minutes)...");

https.get(MODEL_URL, (res) => {
  const file = fs.createWriteStream(modelPath);
  res.pipe(file);
  file.on("finish", () => {
    file.close();
    console.log("✅ Mistral model downloaded.");
    registerModel();
  });
});

function registerModel() {
  const registryPath = path.join(".continuum", "agents.json");
  const registry = fs.existsSync(registryPath)
    ? JSON.parse(fs.readFileSync(registryPath, "utf8"))
    : { agents: [] };

  registry.agents.push({
    id: "mistral-7b",
    name: "Mistral 7B",
    provider: "local",
    capabilities: ["chat", "code", "generate"]
  });

  fs.writeFileSync(registryPath, JSON.stringify(registry, null, 2));
  console.log("🤖 Mistral registered as local agent.");
}
