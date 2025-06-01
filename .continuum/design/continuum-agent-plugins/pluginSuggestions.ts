
// pluginSuggestions.ts - Recommend models by project + hardware

import { evaluateHardware } from './hardware';

export function suggestModels(projectType: string) {
  const hw = evaluateHardware();
  const suggestions = [];

  if (hw.score < 3) {
    suggestions.push("llama-cpp");
  } else if (hw.score >= 6) {
    suggestions.push("mistral-gpu");
  }

  if (projectType.includes("code") || projectType.includes("engineering")) {
    suggestions.push("gpt4o");
  }

  if (projectType.includes("writing") || projectType.includes("research")) {
    suggestions.push("claude-haiku");
  }

  return suggestions;
}
