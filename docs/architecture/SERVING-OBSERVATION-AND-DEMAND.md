# Serving observations and unmet demand

Owner: card 7028c695, Codex helper. Source design before edits.

The current registry stores a substituted growth sample in `sent_peak`: if a turn's untrimmed candidate pool exceeds its window, the sample is that entire pool. `sent_median_with` and the leased-in ring then feed both growth planning and `typical_prompt_tokens`, which rejects remembered geometry. A refused leased-in prompt also enters the same ring as a successful server usage count. These are distinct facts.

Measured Windows6218: stored62720/1 with typical2866096; boot6144, then32000/1,45312/1,51968/2. The 2.86M value includes all unfitted room history and all offered grounding, not admitted context. It remains a requested growth signal; it cannot certify that a lane served a 2.86M prompt.

## Shared ownership

- Keep the existing working-set registry, resource-admission sample ring, ServingDemand and served-window store. No new manager, watcher, timer or cap.
- Introduce one typed distinction for requested demand versus successful served prompt evidence. The existing adapter records successful service at explicit terminal completion while TurnAdmission still holds the endpoint generation read lease. EndpointAdmission verifies the response model against its owned launch contract, whose model ID also supplies the native `--alias`. The normalized endpoint and canonical model travel with the observation; a later persona or adapter binding cannot relabel it. Unknown generation/model or zero usage is no observation.
- Preserve starvation-aware growth target and refusal demand. Local growth remains in the existing registry. The existing admission sample owner retains Requested and Served history independently at its existing history depth per role: a burst of successful small prompts cannot evict refusal demand. A caller-side leased-in count contributes growth only; it cannot retrospectively certify the generation that already released its lease.
- Carry both meanings through the existing plan input once. Growth remains responsible for increasing capacity subject to the existing host, model, neighbor and requirement constraints. It never overwrites observed service evidence. Live and remembered service both provide the same fallback only when growth evidence is absent.
- Persist observed evidence additively. Legacy mixed sent values remain requested demand only. Legacy typical values are ambiguous, so they cannot disqualify actual recorded window/lanes or impersonate accepted service. The store records new evidence provenance explicitly; rollback fields receive only safe actual values. No manual state rewrite.

## Regression requirements

Use existing registry/plan/store fixtures. Reproduce starved2048 + requested56057 + actual828: growth remains above2048; restart retains an actual known62720 geometry despite requested2866096. A valid observed prompt larger than a subsequent collapsed geometry must still reject that collapse. Mixed Requested/Served samples must preserve refused demand without treating refusal as service. Legacy contaminated JSON loads with unknown served evidence and intact demand/geometry. Foreign model records and neighboring resident/budget admission remain isolated.

Not claimed: 51968 alone explains coding behavior, or the precise initial5965 allocation budget has been fully diagnosed. This fixes the proven semantic conflation; source review and hosted tests precede deployment.

## Native usage semantics

OpenAI wire prompt_tokens includes full task tokens (`server-context.cpp`: n_prompt_tokens=task.n_tokens(); `server-task.cpp`: usage_json_oaicompat), with cached prefix reported separately in prompt_tokens_details.cached_tokens. The adapter consumes prompt_tokens, not evaluated-suffix timing. The 2.86M candidate pool remains an existing growth request, not an established useful-context requirement. No new per-persona observed store or schema is needed.
