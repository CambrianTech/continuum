# Post draft: "We served Qwen3.8-Flash-Next on a 64GB MacBook Pro. Receipts inside."

*(For the local-AI threads — Wesche's comparison thread, the "local not ready"
skeptics, AtomicChat's M64 drop. Thread-reply format; each ¶ can stand alone.)*

---

Qwen3.8-Flash-Next (176B MoE, 6B active) is running on our M5 Pro 64GB —
possibly the first local serving anywhere (the llama.cpp PR is still open; we
merged it into our fork). What it actually took, with receipts:

**The trick is in the shards.** The 28-shard M64 artifact is 79GB — but shard 2
is ONE tensor: `per_layer_token_embd`, a 35.8GB n-gram lookup table that is
*designed* to stay on disk. Pin it off-GPU (`-ot "per_layer_token_embd.*=CPU"`
— lookups are memory reads, not matmul, so inference stays GPU-only) and the
43GB of compute weights fit Metal. Miss that and you get instant
`kIOGPUCommandBufferCallbackErrorOutOfMemory`.

**Auto-fit lies about this model.** llama's `-fit` heuristics count the pinned
table as loadable and mis-size everything. `-fit off`, explicit geometry,
`--no-warmup` (warmup would fault the whole 36GB table into RAM), ubatch 512.

**Serving ≠ ready.** First request on a fresh server must be a tiny raw
completion — a chat-sized first request OOMs during cold graph build and the
backend latches dead while `/health` stays green. Our lane's readiness smoke
probe doubles as this warmup.

**The honest numbers** (measured, idle box, same 31k-token prompt as our
incumbent): 23 tok/s short, 16.6–17.5 tok/s at depth, vs Ornith-1.5-35B-A3B's
40 tok/s. So the skeptics are half right: at naive geometry this class of model
is a slow brain locally. But the ceiling isn't physics — an external receipt
this week showed the same model at ~40 tok/s in 37GB with 60% of experts
streaming from SSD. That's a *paging* problem, and expert-paging machinery
(table-driven gather, residency cache, container streaming) is what we build.
"Local not ready" is a scheduling problem wearing a physics costume.

Every number above has a commit or a probe trail behind it. The whole system —
teams of continuously-learning personas on consumer hardware, benchmarked on
the same public suites as the leaderboards — is open:
github.com/CambrianTech/continuum

---

*(Optional closer for the sub-thread about $20 subs vs $900 GPUs:)* the actual
answer is the laptop you already own. 64GB of unified memory serves a 176B-MoE
today, badly, and a 35B-A3B at 40 tok/s, well — and the gap between those two
is software that's being written in public.
