
// hardware.ts - Evaluate system capability for local model usage

import os from 'os';

export function evaluateHardware() {
  const cpus = os.cpus();
  const totalMem = os.totalmem();
  const hasGPU = false; // GPU detection stub

  const cpuScore = Math.min(10, cpus.length * (cpus[0].speed / 1000));
  const memGB = totalMem / 1e9;
  const memScore = Math.min(10, memGB / 2);
  const score = (cpuScore + memScore) / 2;

  return {
    cpu: cpus[0].model,
    cores: cpus.length,
    ram: `${Math.round(memGB)} GB`,
    score: Number(score.toFixed(1)),
    recommendation: score < 5 ? "Use Root and Lumen (avoid local LLaMA)" : "Local models supported"
  };
}
