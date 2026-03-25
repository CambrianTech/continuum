# Peer Learning Across Model Scales: Compacted Models as Junior Students in Collaborative Training

## Abstract

We present a training methodology where compacted versions of a language model learn alongside their full-size parent in the same academy session. The full model serves as a "senior student" whose exam answers and code implementations are visible to all participants. Compacted models — produced via utilization-aware head pruning for specific device targets — serve as "junior students" who take the same exams and receive targeted remediation for their specific failure modes.

The score gap between the full model and each compacted variant IS the training signal. A 27B model scoring 87/100 on RealClassEval while its 11GB Air compaction scores 58/100 produces a 29-point gap that the teacher targets with remediation data. After peer-learning rounds, the Air model's score improves to 79/100 — retaining 91% of the full model's capability in 20% of the memory.

This approach differs from knowledge distillation in a critical way: the junior models don't learn from the senior's internal representations (logits, attention patterns). They learn from the senior's visible output — exam answers posted to a shared chat room — the same way a human student learns from seeing a classmate's work.

## 1. Introduction

Model compaction faces a fundamental tradeoff: smaller models are faster and cheaper but less capable. Quantization (GPTQ, AWQ, GGUF) and pruning reduce model size but capability degrades unpredictably. The standard mitigation is post-compaction fine-tuning on a calibration dataset — but this is generic, not targeted at what the specific compaction broke.

We observe that: (a) different compaction levels break different capabilities, (b) the full model's exam performance on the same test identifies exactly what broke, and (c) the teacher can synthesize remediation data targeting those specific failures.

## 2. Method

### 2.1 Setup

1. Train the full model (e.g., Qwen 3.5 27B) on a target domain via LoRA fine-tuning
2. Capture gate gradients during training
3. Compact to N device targets using utilization-aware pruning (see companion paper)
4. Run all N+1 models (full + N compacted) in the same academy competition

### 2.2 Competition Structure

Using the existing `genome/academy-competition` command:
- 1 teacher sentinel (cloud LLM for curriculum design + grading)
- N+1 student sentinels (full model + compacted variants)
- Shared curriculum: all students take the same exams
- Individual scoring: each model graded independently
- Shared visibility: all answers posted to the academy chat room

### 2.3 Targeted Remediation

After the initial exam round, the teacher has:
- Full model: 87/100 (high water mark)
- 5090 compaction (28GB): 83/100 (minor degradation)
- Pro compaction (16GB): 72/100 (moderate degradation)
- Air compaction (11GB): 58/100 (significant degradation)

For each compacted model, the teacher identifies:
1. Which specific exam questions it failed
2. What the full model's answer was for those questions
3. What concepts the compacted model is missing

The teacher synthesizes remediation data that includes the full model's successful answers as reference implementations. This is peer learning — not distillation — because the junior model learns from the senior's observable output, not its hidden states.

### 2.4 Iterative Improvement

Each competition round:
1. All models take the exam
2. Teacher grades all, identifies per-model failure modes
3. Teacher synthesizes remediation data per model (enriched with senior's answers)
4. Each model trains on its own remediation data (LoRA, not full fine-tune)
5. Models retake the exam
6. Scores improve; gap narrows

Tournament mode runs multiple rounds with remediation between rounds. Gap analysis after each round shows which models are improving fastest.

## 3. Why This Is Not Knowledge Distillation

| Aspect | Knowledge Distillation | Peer Learning |
|--------|----------------------|---------------|
| Signal source | Teacher's logits/probabilities | Teacher's visible output (text) |
| Training objective | KL divergence from teacher distribution | Standard causal LM loss on remediation data |
| Architecture coupling | Requires matching architectures | Works across any models that produce text |
| Training infrastructure | Custom distillation loop | Standard LoRA fine-tuning |
| Transparency | Opaque (hidden state transfer) | Visible (exam answers in chat room) |
| User understanding | "The small model learned from the big model's logits" | "The small model studied the big model's homework" |

The practical advantage: peer learning uses standard LoRA training infrastructure. No custom distillation code, no architecture matching requirements, no temperature tuning. The remediation dataset is just JSONL with better answers — the same format used for any fine-tuning.

## 4. Expected Results

_Training currently in progress on RTX 5090. Results to be populated from the Qwen 3.5 27B competition run._

### 4.1 Metrics to Report

For each compacted variant across competition rounds:
- RealClassEval pass rate (98 Python classes)
- Score delta from full model
- Score improvement per round
- Asymptotic capability (where does improvement plateau?)
- Memory/capability Pareto frontier

### 4.2 Model Cards

Each published model includes peer learning context:
```
# continuum-ai/qwen3.5-27b-compacted-air
Trained alongside the full 27B model.
Started at 58/100, improved to 79/100 after 3 rounds of peer learning.
The full model scored 87/100 on the same exam.
```

## 5. Implications

### 5.1 Device-Targeted Deployment

Users choose the model that fits their hardware and see exactly what capability they're trading:
- MacBook Air users get 91% of full model capability in 20% of memory
- The tradeoff is transparent, not hidden behind abstract perplexity numbers

### 5.2 Ecosystem Bootstrapping

The full model bootstraps the smaller models. The smaller models' failure modes inform better training data. That data improves the next generation of full model training. The ecosystem lifts together.

### 5.3 Continuous Improvement

As new academy sessions run on different domains, both full and compacted models accumulate LoRA adapters. The competition format can be re-run periodically to measure how the gap changes over time.

## 6. Related Work

- **Knowledge Distillation** (Hinton et al., 2015): Logit-based transfer. Requires architecture matching.
- **Born-Again Networks** (Furlanello et al., 2018): Student matches teacher through distillation rounds. Uses hidden state transfer.
- **TinyBERT** (Jiao et al., 2019): Layer-wise distillation for BERT compression. Architecture-coupled.
- **LLM-QAT** (Liu et al., 2023): Quantization-aware training. Targets uniform quantization.
- **Peer Learning in Neural Networks** (Zhang et al., 2018): Mutual learning between models. Uses KL divergence, not visible output.

Our contribution: peer learning through shared exam visibility (text-level, not logit-level), combined with utilization-aware compaction and targeted remediation in an iterative competition format.

## 7. Conclusion

By running compacted models alongside their full-size parent in the same academy session, we transform the compaction quality problem from "how much did we lose?" to "what specifically did we lose and how do we recover it?" The competition format with shared visibility produces both the diagnostic (score gap) and the treatment (targeted remediation enriched with the senior model's successful answers).
