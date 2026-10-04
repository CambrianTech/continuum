# World models (DeepMind, LeCun/Meta) vs the event mind

2026-10-04. Joel's question: we are stuck with LLMs for now, but we turn them into world models the way a base model without vision gets a vision bridge: a bridge in front of the model gives the mind a live, typed state of the world and a prediction to be surprised against. How does that compare with the published world-model work from DeepMind and LeCun/Meta? This page is a comparison, not a reading list, and it is as plain about where we are ahead as about where we are behind. It compares against [`docs/architecture/EVENT-MIND.md`](../architecture/EVENT-MIND.md). Bracketed numbers are sources; "unverified" marks what could not be confirmed from a primary source.

## The core difference

DeepMind and Meta **learn** the world's state and dynamics from pixels, because the physical world is only partly observable. Dreamer learns a latent state and predicts the result of actions [1][2]. JEPA predicts embeddings of the masked or future parts of its input, not pixels [3][4][5]. Genie generates the next frame from past frames and actions [6][7]. The event mind takes a different route. A software world (rooms, boards, builds, reviews) can be **observed exactly**, so the state is engineered rather than learned: one typed, event-fed ViewState per activity. The LLM is the predictor, and its prediction is a free-text `Expectation`. The comparison follows from that one choice.

## 1. How the mechanisms map

| event mind | closest published mechanism | fit | where it breaks |
|---|---|---|---|
| One truth per activity | Dreamer's latent state [1]; the perception module in LeCun's paper [5] | Medium | Their state is an *estimate*, learned and uncertain. Ours is exact and shared with the human's screen. Our state has no uncertainty to model, but nothing generalizes from it either: it covers only the views someone registered. |
| Delta above the cursor | Dreamer's posterior update from each new observation [1] | Loose | A cursor is bookkeeping (what she has read). It is not inference. Nothing in the delta says what she should have expected. |
| Salience as prediction error | ICM: curiosity is prediction error in a learned feature space [8]; Plan2Explore [9] | Close in intent, weak in substance | In their work the error is a *number* (a loss in latent space). Ours is `Surprise { expected: text, observed: String }`, so an LLM judges the mismatch. Nothing measures it. Raw prediction error is also known to fail in stochastic settings, the "noisy TV" problem [10]. Plan2Explore uses ensemble *disagreement* because it falls once something has been learned [9]. Our acceptance rule ("surprised by the world, not by noise") handles one kind of noise, her own misrouted acts. It does not handle a world that is just random (flaky CI, a teammate's mood). |
| Surprise as curriculum | Prioritized experience replay, which replays high-error transitions more often [11] | Close | Replay re-weights updates to one model trained continuously. Our genes are separate LoRA adapters trained in batches. Nothing published shows that LoRA trained on surprising turns lowers *that* surprise later. LoRA also learns less than full finetuning, though it forgets less [12]. |
| Gene paging on attention switches | The configurator in LeCun's paper, which "pre-configures" perception, world model, cost and actor for the task [5] (via [13]) | Close | The configurator is a learned controller with no implementation behind it. Gene paging is a concrete LRU over adapters, but it has no learned policy for *which* gene fits. |
| Amygdala tuning the dial | LeCun's intrinsic cost: hard-wired, "similar to that of the amygdala" (APTAMI p. 14, quoted in [13]) | Close in name | His intrinsic cost is the objective the agent minimizes. Ours (EVENT-MIND.md §7) is a fast valuation of threat and urgency that raises arousal, widens her dial and promotes salience to `Urgent`, then decays; it "never acts; it tunes attention". It sets what makes her look up, not what she optimizes: a different mechanism, and arguably the safer one, because it cannot become a reward to game. |
| Regions subscribing on a mind bus | LeCun's modules (perception, world model, cost, actor, configurator, memory) [5] | Structure only | In every system cited here the modules are one differentiable graph trained end to end. Ours (EVENT-MIND.md §7: admission, recall, genome, curriculum, amygdala, governor) are separate CBAR regions subscribing to typed mind events, each with its own task and cadence. Theirs can learn the interfaces between modules; ours can add, replace or inspect a region without retraining anything. |

## 2. What they measure that we should

- **Prediction error over time, per activity.** Every one of these systems reports a falling prediction loss. We have no number to plot until `Expectation` gets a quantity. One cheap option (my proposal, not from the literature) is to score the observed event's negative log-likelihood under the serving LLM, given her continuation. That turns surprise into a measurement instead of a judgment.
- **Disagreement, not just error** [9]. If two samples, or two genes, disagree about what comes next, that is epistemic surprise, which training can fix. Error on its own includes noise that no gene will ever fix.
- **Surprise falling after a gene lands, plus retention.** Our own metric, but it needs the controls they use. Measure the same surprise class before and after the gene. Check out-of-domain performance for forgetting [12]. Count turns per gain (Dreamer's whole case is sample efficiency on fixed settings across 150+ tasks [2]).
- **Horizon and consistency.** DreamerV3 reports how far ahead it predicts open-loop, and Genie reports how long a world stays consistent (Genie 2: up to a minute; Genie 3: a few minutes) [1][6][7]. Our equivalents: how long a continuation's expectation stays correct, and the restart-resume rate (the 76-of-80 figure).

## 3. Where each side is genuinely stronger

**Where an event-fed world view in front of an existing LLM wins**

- **No retraining of the base.** CWM, Meta's nearest move on code, puts the world model *into* the LLM by mid-training a 32B model on interpreter and Docker trajectories [14]. That is a lab-scale training run. A bridge in front of a frozen base, with LoRA genes, can be built on a consumer GPU. The project needs to *adapt* LLMs, and none of the world-model work above offers a path for that.
- **The state is exact, typed and shared.** A queue in front of a model passes along copies of messages and leaves the model to rebuild the state. Our ViewState *is* the state: it is the one the human sees, it cannot drift from it, and it costs nothing while quiet. Dreamer and JEPA spend most of their capacity estimating state that we can simply read.
- **Many activities at once.** The published agents act in one environment per episode. A strip of all her activities, ranked by salience, covers ground that their benchmarks do not test.
- **Legibility.** Every salience reason, wake and switch is a typed event with a probe. A learned latent cannot be inspected that way.

**Where their approach is genuinely stronger**

- **Calibrated, quantitative prediction.** A latent loss is a number that training drives down. Our expectation is prose and our surprise is an LLM's opinion. Until that changes, "salience as prediction error" is an analogy, not a mechanism.
- **Planning by imagined rollouts.** MuZero plans with a learned model of reward, policy and value [15]. Dreamer 4 trains its whole policy inside the model, from offline data only [16]. V-JEPA 2-AC plans robot grasps zero-shot from less than 62 hours of robot video [3]. The event mind cannot simulate "if I push this, the review will…" before acting. Its only prediction is one sentence.
- **Generalization.** A learned world model covers states nobody registered a view for. Ours covers only registered ViewState kinds.
- **Perception.** JEPA's video encoders see the physical world, and V-JEPA 2 has already been aligned with an LLM for video QA [3]. If personas need physical or visual grounding, the encoder is the gene to graft, not something to rebuild.

**Their weaknesses for our purpose, stated plainly.** Genie's consistency lasts minutes, not the days a project runs [7]. JEPA and Dreamer have no language-native interface for an LLM agent's text-and-tool world. And none of them tackles continual adaptation of a frozen LLM, which is our actual problem.

## Bottom line

This is not a catch-up list. For the problem we actually have, adapting a frozen LLM to a world we can read exactly, the event mind is ahead of the published work: none of it offers continual adaptation of a frozen LLM, none has a language-native state an agent and a human share, and none runs one mind across many activities at once. It already has the *architecture* LeCun described (perception, world state, configurator, intrinsic cost, short-term memory [5]), built from inspectable parts instead of one trained graph.

What it should take from them is one *quantity*: a numeric prediction error, separated from noise (disagreement, not raw error [9][10]) and tracked over time per activity. Today `Surprise { expected, observed }` is an LLM's judgment of two strings. Make the expectation scorable, and the project's own metric, surprise falling after a gene lands, becomes a real learning curve, measured with their controls for forgetting [12] and sample efficiency [2]. Where they are genuinely stronger (planning by imagined rollouts, generalizing past registered views, physical perception), the event mind can graft their pieces as genes, such as a V-JEPA encoder for vision [3], rather than rebuild them.

## Sources

1. Hafner et al., "Mastering diverse control tasks through world models" (DreamerV3), *Nature* 640 (2025). https://www.nature.com/articles/s41586-025-08744-2
2. DreamerV3 preprint, arXiv:2301.04104. https://arxiv.org/abs/2301.04104
3. Assran et al., "V-JEPA 2", arXiv:2506.09985. https://arxiv.org/abs/2506.09985
4. Assran et al., "I-JEPA", arXiv:2301.08243 (CVPR 2023). https://arxiv.org/abs/2301.08243
5. LeCun, "A Path Towards Autonomous Machine Intelligence" v0.9.2 (2022). https://openreview.net/pdf?id=BZ5a1r-kVsf (I could not load the full text; module descriptions are confirmed via [13] and secondary summaries)
6. Google DeepMind, "Genie 2". https://deepmind.google/discover/blog/genie-2-a-large-scale-foundation-world-model/
7. Google DeepMind, "Genie 3: A new frontier for world models". https://deepmind.google/blog/genie-3-a-new-frontier-for-world-models/
8. Pathak et al., "Curiosity-driven Exploration by Self-supervised Prediction", arXiv:1705.05363. https://arxiv.org/abs/1705.05363
9. Sekar et al., "Planning to Explore via Self-Supervised World Models", arXiv:2005.05960. https://arxiv.org/abs/2005.05960
10. Burda et al., "Large-Scale Study of Curiosity-Driven Learning", arXiv:1808.04355. https://arxiv.org/abs/1808.04355
11. Schaul et al., "Prioritized Experience Replay", arXiv:1511.05952. https://arxiv.org/abs/1511.05952
12. Biderman et al., "LoRA Learns Less and Forgets Less", arXiv:2405.09673. https://arxiv.org/abs/2405.09673
13. Byrnes, "LeCun's 'A Path Towards Autonomous Machine Intelligence' has an unsolved technical alignment problem" (quotes APTAMI p. 14). https://www.alignmentforum.org/posts/C5guLAx7ieQoowv3d/
14. Meta FAIR, "CWM: An Open-Weights LLM for Research on Code Generation with World Models", arXiv:2510.02387. https://arxiv.org/abs/2510.02387
15. Schrittwieser et al., "Mastering Atari, Go, chess and shogi by planning with a learned model" (MuZero), *Nature* 588 (2020); arXiv:1911.08265. https://arxiv.org/abs/1911.08265
16. Hafner, Yan, Lillicrap, "Training Agents Inside of Scalable World Models" (Dreamer 4), arXiv:2509.24527. https://arxiv.org/abs/2509.24527

**Unverified:** H-JEPA details beyond the summaries of [5] (I could not read the full paper, which was behind OpenReview's verification page). The DreamerV3 open-loop horizon figure (45 frames from 5 context images) appeared in the search summary of the Nature page, which I could not open directly; it is not used as a number above. I found no peer-reviewed 2026 world-model agent paper from DeepMind or Meta beyond Dreamer 4 and CWM. One blog's claim that "V-JEPA 2 was released January 2026" contradicts the June 2025 arXiv date and was discarded.
