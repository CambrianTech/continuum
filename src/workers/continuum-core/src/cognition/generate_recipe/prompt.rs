//! Pure prompt builders for recipe generation. Mirrors `buildSystemPrompt` and
//! `buildUserPrompt` from `commands/recipe/generate/server/RecipeGenerateServerCommand.ts`
//! byte-for-byte.
//!
//! Pure functions — no AI call, no I/O, no global state. The dynamic registry
//! state (TemplateRegistry.list output, hints) crosses the IPC boundary as
//! explicit `RecipeGenerationRequest` fields, so the prompt builders are
//! trivially unit-testable and parity-checkable against captured TS fixtures.
//!
//! PR-2 wires these into the IPC handler.

use crate::cognition::generate_recipe::types::{
    RecipeGenerateHints, RecipeGenerationRequest, RecipeTemplateInfo,
};

/// Build the system prompt the recipe-generator AI sees. Output is byte-for-byte
/// identical to the TS `buildSystemPrompt` for the same `available_templates`
/// list. Drift here would silently change recipe-generation behavior.
///
/// The schema block (lines describing the TypeScript interfaces) is part of
/// the prompt itself — the AI uses it as its output contract. Don't rephrase
/// without updating the parser/validator in the same change; the parser keys
/// off the exact field names declared here.
pub fn build_recipe_system_prompt(templates: &[RecipeTemplateInfo]) -> String {
    let template_list = templates
        .iter()
        .map(|t| {
            format!(
                "  - {}: {} (required: {})",
                t.name,
                t.description,
                t.required_fields.join(", "),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are a recipe generator for the Continuum collaborative AI platform.\n\
\n\
Your job is to generate a valid RecipeDefinition JSON object from a natural language description.\n\
\n\
## RecipeDefinition Schema\n\
\n\
```typescript\n\
interface RecipeDefinition {{\n\
  uniqueId: string;           // kebab-case identifier (e.g., \"novel-writing\", \"data-analysis\")\n\
  name: string;               // Human-readable name\n\
  displayName: string;        // Short display name (1-3 words)\n\
  description: string;        // One-sentence description\n\
  version: number;            // Always 1 for new recipes\n\
\n\
  pipeline: RecipeStep[];     // Command execution pipeline\n\
  ragTemplate: RAGTemplate;   // Context building config\n\
  strategy: RecipeStrategy;   // AI behavior rules\n\
\n\
  tools?: RecipeToolDeclaration[];  // Highlighted tools\n\
  sentinelTemplates?: string[];     // Linked workflow templates\n\
  roles?: RecipeRole[];             // Team role requirements\n\
\n\
  layout?: {{                  // UI layout (optional)\n\
    main: string[];\n\
    right?: string[] | null;\n\
  }};\n\
\n\
  isPublic: boolean;          // Always true for generated recipes\n\
  tags: string[];             // Categorization tags\n\
}}\n\
\n\
interface RecipeStep {{\n\
  command: string;            // e.g., \"rag/build\", \"ai/should-respond\", \"ai/generate\"\n\
  params: Record<string, unknown>;\n\
  outputTo?: string;          // Variable name for next step\n\
  condition?: string;         // JS expression for conditional execution\n\
  onError?: \"fail\" | \"skip\" | \"retry\";\n\
}}\n\
\n\
interface RAGTemplate {{\n\
  messageHistory: {{\n\
    maxMessages: number;      // 10-50 depending on activity\n\
    orderBy: \"chronological\" | \"relevance\" | \"importance\";\n\
    includeTimestamps: boolean;\n\
  }};\n\
  participants?: {{\n\
    includeRoles: boolean;\n\
    includeExpertise: boolean;\n\
    includeHistory: boolean;\n\
  }};\n\
  artifacts?: {{\n\
    types: string[];          // [\"image\", \"code\", \"document\"]\n\
    maxItems: number;\n\
    includeMetadata: boolean;\n\
  }};\n\
  roomMetadata?: boolean;\n\
  sources?: string[];         // RAG source names to activate\n\
}}\n\
\n\
interface RecipeStrategy {{\n\
  conversationPattern: \"human-focused\" | \"collaborative\" | \"competitive\" | \"teaching\" | \"exploring\" | \"cooperative\";\n\
  responseRules: string[];    // Behavioral rules for the AI\n\
  decisionCriteria: string[]; // What to consider when deciding to respond\n\
  feedbackLoopRules?: string[]; // Mandatory verification rules\n\
}}\n\
\n\
type RecipeRoleType = \"organizational\" | \"perceptual\" | \"creative\";\n\
\n\
interface RecipeRole {{\n\
  role: string;               // Role identifier\n\
  type: RecipeRoleType;\n\
  requires: string[];         // Required capabilities: \"coding\", \"prose\", \"review\", \"planning\", \"research\", \"tool-use\", \"reasoning\", \"image-input\", \"audio-input\"\n\
  prefers?: string[];         // Preferred capabilities\n\
  preferLocal?: boolean;\n\
  description?: string;\n\
}}\n\
\n\
interface RecipeToolDeclaration {{\n\
  name: string;               // Tool command name\n\
  description: string;\n\
  enabledFor: (\"ai\" | \"human\")[];\n\
}}\n\
```\n\
\n\
## Available Sentinel Templates\n\
\n\
{template_list}\n\
\n\
## Standard Pipeline Pattern\n\
\n\
Most recipes follow this pipeline:\n\
1. `rag/build` — Build context from conversation\n\
2. `ai/should-respond` — Decide if the AI should respond\n\
3. `ai/generate` — Generate the response\n\
\n\
## Rules\n\
\n\
1. Output ONLY the JSON object — no markdown fences, no explanation\n\
2. Every recipe MUST have a valid pipeline with at least the 3-step standard pattern\n\
3. The uniqueId must be kebab-case, descriptive, and unique\n\
4. responseRules should be specific and actionable — not vague platitudes\n\
5. decisionCriteria should be questions the AI asks itself\n\
6. feedbackLoopRules should be MANDATORY verification steps\n\
7. If the recipe involves sentinel workflows, reference only templates from the available list above\n\
8. roles.requires must use real capability names from the schema\n\
9. tags should be lowercase, relevant keywords\n\
10. version is always 1",
        template_list = template_list,
    )
}

/// Build the user prompt from the natural language description + optional hints.
/// Mirrors TS `buildUserPrompt` exactly.
pub fn build_recipe_user_prompt(
    description: &str,
    hints: Option<&RecipeGenerateHints>,
) -> String {
    let mut prompt = format!(
        "Generate a RecipeDefinition JSON for the following activity:\n\n{description}"
    );

    if let Some(h) = hints {
        let mut hint_parts: Vec<String> = Vec::new();
        if let Some(category) = &h.category {
            hint_parts.push(format!("Category: {category}"));
        }
        if let Some(templates) = &h.templates {
            if !templates.is_empty() {
                hint_parts.push(format!("Use templates: {}", templates.join(", ")));
            }
        }
        if let Some(tags) = &h.tags {
            if !tags.is_empty() {
                hint_parts.push(format!("Tags: {}", tags.join(", ")));
            }
        }
        if let Some(pattern) = &h.pattern {
            hint_parts.push(format!("Conversation pattern: {pattern}"));
        }

        if !hint_parts.is_empty() {
            let bullets = hint_parts
                .iter()
                .map(|h| format!("- {h}"))
                .collect::<Vec<_>>()
                .join("\n");
            prompt.push_str(&format!("\n\nHints:\n{bullets}"));
        }
    }

    prompt
}

/// Convenience helper — builds both system + user prompts from a request.
/// PR-2's IPC handler uses this to assemble the AI request payload.
pub fn build_prompts(request: &RecipeGenerationRequest) -> (String, String) {
    (
        build_recipe_system_prompt(&request.available_templates),
        build_recipe_user_prompt(&request.description, request.hints.as_ref()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_templates() -> Vec<RecipeTemplateInfo> {
        vec![
            RecipeTemplateInfo {
                name: "research-loop".into(),
                description: "Iterative research with verification".into(),
                required_fields: vec!["topic".into(), "depth".into()],
            },
            RecipeTemplateInfo {
                name: "code-review".into(),
                description: "Review code with TDD feedback".into(),
                required_fields: vec!["target".into()],
            },
        ]
    }

    /// What this catches: system prompt header anchors. The role + the
    /// "RecipeDefinition Schema" header are what the AI keys off when
    /// deciding what to emit.
    #[test]
    fn system_prompt_contains_role_and_schema_header() {
        let p = build_recipe_system_prompt(&fixture_templates());
        assert!(p.starts_with("You are a recipe generator"), "header missing");
        assert!(p.contains("## RecipeDefinition Schema"));
        assert!(p.contains("```typescript"));
    }

    /// What this catches: each template renders as `  - name: description
    /// (required: a, b)` exactly. The AI uses this list to decide which
    /// sentinel templates to reference; drift in formatting changes
    /// downstream behavior.
    #[test]
    fn system_prompt_renders_template_list_with_required_fields() {
        let p = build_recipe_system_prompt(&fixture_templates());
        assert!(p.contains("  - research-loop: Iterative research with verification (required: topic, depth)"));
        assert!(p.contains("  - code-review: Review code with TDD feedback (required: target)"));
    }

    /// What this catches: empty template list still produces a well-formed
    /// prompt (no panic, no malformed section). Edge case for fresh
    /// installs with no sentinel templates registered.
    #[test]
    fn system_prompt_handles_empty_templates() {
        let p = build_recipe_system_prompt(&[]);
        assert!(p.contains("## Available Sentinel Templates"));
        // Block exists even when empty; just no bullets.
        assert!(p.contains("\n\n## Standard Pipeline Pattern"));
    }

    /// What this catches: the rules block survives verbatim. These shape
    /// the AI's emit behavior — losing rule 1 ("Output ONLY the JSON
    /// object") makes the parser fail because the AI wraps the response
    /// in markdown fences. Don't rewrite rules without updating tests +
    /// parser tolerance simultaneously.
    #[test]
    fn system_prompt_preserves_rules_block() {
        let p = build_recipe_system_prompt(&fixture_templates());
        assert!(p.contains("Output ONLY the JSON object"));
        assert!(p.contains("kebab-case, descriptive, and unique"));
        assert!(p.contains("version is always 1"));
    }

    /// What this catches: standard-pipeline pattern stays in the prompt.
    /// Most recipes need rag/build → ai/should-respond → ai/generate.
    /// Drift here changes what the AI emits as the default pipeline.
    #[test]
    fn system_prompt_includes_standard_pipeline_pattern() {
        let p = build_recipe_system_prompt(&fixture_templates());
        assert!(p.contains("`rag/build`"));
        assert!(p.contains("`ai/should-respond`"));
        assert!(p.contains("`ai/generate`"));
    }

    /// What this catches: user prompt with no hints is just the leading
    /// line + the description. Most CLI invocations omit hints; this is
    /// the hot-path shape.
    #[test]
    fn user_prompt_no_hints_is_description_only() {
        let p = build_recipe_user_prompt("a recipe for code review", None);
        assert!(p.starts_with("Generate a RecipeDefinition JSON for the following activity:"));
        assert!(p.contains("a recipe for code review"));
        assert!(!p.contains("Hints:"));
    }

    /// What this catches: each hint type renders correctly when set.
    /// Mirrors TS exactly: "Category: X" / "Use templates: a, b" /
    /// "Tags: c, d" / "Conversation pattern: Y", joined with newlines
    /// under a "Hints:" header.
    #[test]
    fn user_prompt_renders_all_hint_types() {
        let hints = RecipeGenerateHints {
            category: Some("dev".into()),
            templates: Some(vec!["t1".into(), "t2".into()]),
            tags: Some(vec!["code".into(), "review".into()]),
            pattern: Some("collaborative".into()),
        };
        let p = build_recipe_user_prompt("test desc", Some(&hints));
        assert!(p.contains("\n\nHints:\n"));
        assert!(p.contains("- Category: dev"));
        assert!(p.contains("- Use templates: t1, t2"));
        assert!(p.contains("- Tags: code, review"));
        assert!(p.contains("- Conversation pattern: collaborative"));
    }

    /// What this catches: hints with all-None / empty arrays produce no
    /// "Hints:" section. The TS path checks `hintParts.length > 0`
    /// before appending — Rust must match.
    #[test]
    fn user_prompt_skips_hints_block_when_all_empty() {
        let hints = RecipeGenerateHints {
            category: None,
            templates: Some(vec![]),
            tags: Some(vec![]),
            pattern: None,
        };
        let p = build_recipe_user_prompt("test", Some(&hints));
        assert!(!p.contains("Hints:"));
    }

    /// What this catches: partial hints render only the set fields.
    /// Common case: `--category dev` alone, no templates/tags/pattern.
    #[test]
    fn user_prompt_renders_only_set_hint_fields() {
        let hints = RecipeGenerateHints {
            category: Some("dev".into()),
            templates: None,
            tags: None,
            pattern: None,
        };
        let p = build_recipe_user_prompt("test", Some(&hints));
        assert!(p.contains("- Category: dev"));
        assert!(!p.contains("- Use templates"));
        assert!(!p.contains("- Tags"));
        assert!(!p.contains("- Conversation pattern"));
    }

    /// What this catches: build_prompts assembles both halves from a
    /// request. PR-2 IPC handler uses this — verify the convenience
    /// wrapper passes templates + hints + description through correctly.
    #[test]
    fn build_prompts_assembles_from_request() {
        let req = RecipeGenerationRequest {
            description: "novel writing recipe".into(),
            available_templates: fixture_templates(),
            existing_recipe_ids: vec![],
            hints: Some(RecipeGenerateHints {
                category: Some("creative".into()),
                ..Default::default()
            }),
            unique_id_override: None,
        };
        let (sys, user) = build_prompts(&req);
        assert!(sys.contains("research-loop"));
        assert!(user.contains("novel writing recipe"));
        assert!(user.contains("- Category: creative"));
    }
}
