
// run-agent.ts - Starts REPL chat with Root using LLaMA or fallback

import readline from "readline";

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  prompt: "🧠 Root > "
});

console.log("Hi, I’m Root. Ask me anything. Type 'exit' to quit.");
rl.prompt();

rl.on("line", (line) => {
  const input = line.trim();
  if (input === "exit") return rl.close();

  // Simulated output (LLaMA could be called here)
  console.log(`🤖 [Root]: I heard "${input}" — let me think...`);
  setTimeout(() => {
    console.log(`🤖 [Root]: Here's a possible answer for "${input}"`);
    rl.prompt();
  }, 1000);
});
