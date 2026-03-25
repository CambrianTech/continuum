# Peer Learning Across Model Sizes — Compacted Models as Junior Students

## The Insight

The full 27B model is the senior student. Compact it to three device targets. Now each compacted model is a junior student in the same academy session. They take the same exams. The senior shows them how it's done — its answers are visible in the academy chat room. The compacted models learn from both the teacher's curriculum AND the senior student's work.

## How It Works

```
1. Train the full Qwen 3.5 27B on RealClassEval → gate_gradients.json
2. Plasticity compaction → three targets:
   - 27B-5090 (28GB, mixed quant)
   - 27B-pro  (16GB, Q4_K_M)
   - 27B-air  (11GB, Q3_K_S)
3. Run a competition: 1 teacher + 4 students (full + 3 compacted)
4. Same exam, same pytest, different scores:
   - 27B full: 87/100
   - 27B-5090: 83/100
   - 27B-pro:  72/100
   - 27B-air:  58/100
5. That gap IS the training signal
6. Teacher generates remediation targeting where smaller models fail
7. Smaller models train on remediation data + see senior's answers in chat
8. Retake exam. Gap narrows.
9. Publish all four with scores in model cards.
```

## Peer Learning Through Chat

This is NOT weight sharing. It's learning through shared visibility:

- The 27B model's Python implementation is posted to the academy chat room
- The 11GB model's implementation is also posted — side by side
- The teacher grades both, points out the differences
- The 11GB model's next training round includes the 27B's better answers as training examples
- The chat history becomes RAG context for subsequent inference

Three students answering the same question differently, graded by the same teacher, learning from each other's work. Visible in the academy room. That's the product demo.

## Why This Matters for Users

The model card tells the whole story:

```markdown
# continuum-ai/qwen3.5-27b-compacted-air

Compacted from Qwen 3.5 27B for MacBook Air (11GB).

## Peer Learning Results
- Trained alongside the full 27B model and two other compaction targets
- Started at 58/100 on RealClassEval
- After peer-learning + targeted remediation: 79/100
- The full 27B scored 87/100 on the same exam

## What This Means
You get 90% of the 27B's coding ability in 20% of the memory.
Runs on a MacBook Air. No GPU. No API key. No cloud.
```

Users see EXACTLY how much capability they're trading for portability. Not abstract benchmark numbers — real exam scores on real Python classes, compared against the full model on the same test.

## Competition Mode Integration

The existing `genome/academy-competition` command already supports N students on the same curriculum:

```bash
./jtag genome/academy-competition \
  --skill="python-coding" \
  --competitors="full-27b,compacted-5090,compacted-pro,compacted-air" \
  --model="deepseek-chat" \
  --provider="deepseek" \
  --mode="realclasseval"
```

Each competitor gets their own student sentinel. All share the teacher's exam. Rankings computed automatically. Gap analysis shows exactly where each size model struggles.

## The Compounding Effect

Each round of peer learning makes the smaller models better at the specific things that matter. After enough rounds:

- The Air model might score 79/100 where it started at 58
- That 21-point improvement came from targeted training on its exact failure modes
- The training data was enriched by seeing how the larger model solved the same problems
- The LoRA adapter encoding that improvement is tiny (~200MB)
- Someone on a MacBook Air downloads the adapter, gets that 21-point improvement for free

The full model bootstraps the smaller models. The smaller models' training data improves the next generation of full model training. The whole ecosystem lifts together.
