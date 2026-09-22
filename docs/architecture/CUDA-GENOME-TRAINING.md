# Native genome training: shared ownership, backend kernels

CUDA and MLX implement the existing `FineTuningAdapter` contract. Training jobs
keep the existing UUID, coordinator, provenance, completion sentinel, evaluation
and genome-paging path. This change does not create a second learning pipeline.

## Ownership

`genome/fine_tuning/native_jobs.rs` owns process lifetime, handle validation,
queued/running/terminal status, latched cancellation and diagnostic draining for
both native genome adapters. Backend preparation is asynchronous after a handle
is returned. Cancellation kills and reaps a running training child before
releasing its reservation. Backends own preparation and artifact validation.

The recent Mac `forge/train` path remains a separate caller. Its admission and
CUDA admission now use `forge/training_admission.rs`: one guarded VRAM lease from
the same resource authority as serving. The lease remains pinned for the child
lifetime; a wall-clock deadline cannot release memory still occupied by a
trainer. CUDA refuses missing governance or unknown requirements. The existing
standalone MLX caller's ungoverned/unsized behavior is preserved, not endorsed as
equivalent protection. Migrating that behavior and its separate job registry is
follow-up work requiring Mac validation.

The native **genome MLX adapter** is a different caller from `forge/train`:
it shares process ownership here but still has no measured governor admission.
Card `3e752cc6` remains open. This PR must not be read as closing Mac training
memory safety; that adapter needs measured footprint admission before it loads
weights alongside serving.

`cuda_train.py` owns PyTorch/PEFT tensor operations. It plans from actual model
configuration and meta tensors before loading weights. The plan records weights,
optimizer, activations, logits and allocator overhead separately. The governor
admits this request; PyTorch enforces the granted allocator bound. This is an
estimate, not proof of peak fit or a bound on allocations outside PyTorch.
An underestimated job fails explicitly; it does not silently shorten examples,
change the base, or fall back to CPU/full precision.

The CUDA planner chooses a microbatch from governed and physical headroom.
Token-weighted gradient accumulation retains the requested effective batch,
including partial batches, without shortening the sequence window.

Training uses NF4 double quantization, trainable LoRA parameters, checkpointed
activations, and finite loss/gradient checks. Source examples are split before
chunking so overlapping training context does not leak into validation. Each
supervised token is retained exactly once across sequence windows. The HF
revision resolved during planning is reused during loading and recorded. Both
sides of the loss mask use the training chat template with an explicit
`input_ids` result contract. Generation-only thinking prefixes are not used as
training prefixes; actual Qwen tokenizer preparation is checked separately.

## Artifact boundary

MLX produces `MlxAdapterDir`; CUDA produces `PeftAdapterDir`. `forge/export`
receives an explicit checkpoint format (omission retains MLX compatibility).
The custodian transposes MLX weights into PEFT, or validates native PEFT tensor
pairs, rank and base identity. Both then use the existing pinned llama.cpp GGUF
converter. Native PEFT weights are never transposed as though they were MLX.
The PEFT boundary currently accepts positive integer `lora_alpha` only, matching
our typed training request; float-valued JSON alpha is refused. The GPU fixture
asserts the saved PEFT base identity exactly matches the trainer's resolved base,
so library changes that normalize it differently fail before export acceptance.

The completion sentinel still requires conversion and the existing evaluation
and adoption gates. Completed training alone is not evidence of improvement.
Cross-node conversion requires a custodian that understands the new format;
older custodians cannot handle a PEFT checkpoint.

## Provisioning

Use a dedicated Python 3.12 environment at
`~/.continuum/tools/cuda-training`, or set the existing config environment key
`CUDA_TRAIN_PYTHON` to another interpreter. Windows uses `Scripts/python.exe`;
Linux uses `bin/python`. Install CUDA PyTorch from its official wheel index and
the pinned backend requirements from
`core/continuum-core/src/genome/fine_tuning/cuda-requirements.txt`:

```sh
uv venv --python 3.12 ~/.continuum/tools/cuda-training
# Replace PYTHON with that environment's platform-specific interpreter.
uv pip install --python PYTHON torch==2.11.0 --index-url https://download.pytorch.org/whl/cu128
uv pip install --python PYTHON -r core/continuum-core/src/genome/fine_tuning/cuda-requirements.txt
```

The backend never installs packages during a persona turn. Automatic provision
through the existing tool installer is not implemented in this change.

## Verification and remaining acceptance

The opt-in `CONTINUUM_TEST_CUDA=1 python -m unittest cuda_train_test -v` test uses
locally constructed small Qwen2 and Qwen3.5 hybrid/multimodal architectures. It exercises real CUDA NF4/LoRA,
checks changed adapter weights, unchanged base weights, supervised-token
coverage and validation output. `CONTINUUM_TEST_GGUF=1` additionally runs the
pinned upstream converter for both produced adapters and reads the resulting
GGUF tensor tables. CPU-only data-contract tests run in the existing Rust CI job. This is a mechanics test, not a benchmark or
evidence that Kimi learned. Rust regressions cover native cancellation/foreign
handles and the existing MLX-to-PEFT conversion plus native PEFT validation.

Kimi's registered Qwen3.8-27B source resolves through the existing catalog. An
initial full-batch plan exceeded the 5090's 32 GB. After accounting for the
unquantized output head and choosing microbatch one/effective batch four, the
full-wrapper model-only plan requires 31.04 GB at 2,048 tokens. This is an estimate on an idle
GPU, not simultaneous inference/training acceptance. On the Windows test host,
CUDA reports 31.8 GB free while NVIDIA reports about 25.9 GB used. The live
resource board reports about 6 GB available. The adapter therefore passes the
resource authority's headroom into planning and acquires its lease; CUDA's
memory query alone is not trusted as fleet or even machine-wide availability. A real
curriculum run still needs a capacity-compatible schedule, checked training
footprint, GGUF conversion, isolated evaluation, adoption, and independently
measured transfer. No live inference resident is stopped to make this test fit.

Remote training dispatch, restart
reattachment to native jobs, and typed per-step progress are not implemented
here. The shared owner currently exposes job status and persisted diagnostic
loss records; these limitations must remain visible in acceptance receipts.

Capacity shortage is a queued preparation, not a failed training run. The
planner emits at least the one-example memory plan even with zero headroom,
without loading weights. The native job's preparation waits on the existing
resource daemon's board subscription and retries atomic admission only when
that board changes. Only typed `InsufficientCapacity` waits; other admission
errors still fail. The `training.admission.waiting` probe carries the job's
consumer identity, requested bytes and available bytes, emitting when that
availability changes. The same admission refusal updates the native owner's
existing status watch: `genome/job-status` reports `waiting_for_capacity` with
numeric `requiredBytes` and `availableBytes`. These are the latest admission
observation, not a reservation or a guarantee that the job can eventually fit.
Waiting remains nonterminal in the board and completion sentinel. Cancellation
drops preparation without spawning a trainer.
No additional polling timer or dispatch is created. This does not arrange
serving migration, guarantee fairness between waiting jobs, or recover queued
preparations across a core restart; those remain separate acceptance work.

The Qwen3.5-family regression caught a text-only AutoModel projection taking
precedence over the multimodal wrapper. The trainer now preserves the wrapper;
its config, parameter paths and memory plan include the vision component even
when a curriculum currently contains only text. The fixture carries the actual
base family's MTP configuration; no vendored converter modification is included.
On Windows the hybrid test uses Transformers' reference CUDA operations because
causal-conv1d and flash-linear-attention are absent. Installed versions/absence
and actual model class are recorded in training provenance. Correctness and
GGUF conversion pass; optimized-kernel availability and full-model throughput
remain explicit acceptance work.
