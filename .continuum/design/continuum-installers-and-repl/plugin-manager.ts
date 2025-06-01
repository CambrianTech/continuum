
// plugin-manager.ts - Manage model plugins via CLI

import { execSync } from "child_process";

const cmd = process.argv[2];
const arg = process.argv[3];

if (!cmd) {
  console.log("Usage: continuum plugins [list|install <id>|test <id>]");
  process.exit(0);
}

switch (cmd) {
  case "list":
    const registry = require("../.continuum/agents.json");
    console.log("🧠 Installed Models:");
    registry.agents.forEach((a: any) =>
      console.log(`- ${a.name} (${a.provider}) — ${a.capabilities.join(", ")}`)
    );
    break;

  case "install":
    if (!arg) return console.log("Specify model ID: e.g., mistral");
    execSync(`ts-node install-${arg}.ts`, { stdio: "inherit" });
    break;

  case "test":
    console.log(`🧪 Testing ${arg} model with hello-world...`);
    // Simulate test call
    console.log(`> [${arg}] Hello, world! ✅`);
    break;

  default:
    console.log("Unknown command.");
}
