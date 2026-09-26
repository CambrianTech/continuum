"""Contract tests, plus an explicitly opted-in real CUDA/PEFT kernel test."""
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest
import cuda_train


class CudaTrainingTests(unittest.TestCase):
    def test_chunking_preserves_every_supervised_target_once(self):
        ids = list(range(30))
        rows = list(cuda_train.chunks(ids, 11, 6))
        supervised = [x for row in rows for x in row["labels"][1:] if x != -100]
        self.assertEqual(supervised, ids[11:])
        self.assertTrue(all(len(row["input_ids"]) <= 6 for row in rows))

    def test_no_supervision_is_not_fabricated(self):
        self.assertEqual(list(cuda_train.chunks([1, 2, 3], 3, 2)), [])
        with self.assertRaises(ValueError):
            list(cuda_train.chunks([1, 2], 1, 1))

    @unittest.skipUnless(importlib.util.find_spec("transformers") is not None, "transformers not installed")
    def test_the_chunked_loss_is_the_models_own_loss_without_the_whole_logits_tensor(self):
        # what this catches: the objective and its gradient are unchanged by chunking —
        # a chunk boundary inside the sequence, an ignored-label chunk, and a mean over
        # exactly the supervised targets — while the planner's logits term is bounded
        # to one chunk. CPU, a tiny Qwen2, no CUDA needed.
        import torch
        from transformers import Qwen2Config
        torch.manual_seed(11)
        config = Qwen2Config(vocab_size=64, hidden_size=32, intermediate_size=64, num_hidden_layers=2,
                             num_attention_heads=4, num_key_value_heads=2, max_position_embeddings=64)
        model = cuda_train.model_class(config).from_config(config)
        ids = torch.randint(0, 64, (2, 11))
        labels = ids.clone()
        labels[0, :4] = -100  # a prompt prefix
        labels[1, 6:] = -100  # a chunk with nothing supervised
        batch = {"input_ids": ids, "attention_mask": torch.ones_like(ids), "labels": labels}
        reference = model(**batch).loss
        chunked = cuda_train.chunked_causal_lm_loss(model, batch, chunk=3)
        self.assertTrue(torch.allclose(reference, chunked, atol=1e-5), (reference, chunked))
        reference.backward()
        grad_reference = model.lm_head.weight.grad.clone()
        model.zero_grad(set_to_none=True)
        chunked.backward()
        self.assertTrue(torch.allclose(grad_reference, model.lm_head.weight.grad, atol=1e-5))
        self.assertLess(cuda_train.LOGITS_CHUNK_TOKENS, 4096, "the planner's logits term is one chunk, not a window")
    def test_a_checkpoint_is_written_whole_pointed_at_last_and_the_previous_one_dropped(self):
        # what this catches: the resume reads `checkpoints/LATEST` → a directory holding the
        # adapter and its loop state; the pointer must never name a half-written directory,
        # and only one checkpoint stays on disk.
        class Adapter:
            def __init__(self): self.saved = []
            def save_pretrained(self, path, safe_serialization=True):
                Path(path).mkdir(parents=True, exist_ok=True)
                (Path(path) / "adapter_config.json").write_text("{}", encoding="utf-8")
                self.saved.append(Path(path).name)
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory); adapter = Adapter()
            cuda_train.write_checkpoint(adapter, out, 4, {"step": 4, "epoch": 0, "nextStart": 12, "trainedTokens": 900, "finalLoss": 1.5})
            cuda_train.write_checkpoint(adapter, out, 8, {"step": 8, "epoch": 0, "nextStart": 24, "trainedTokens": 1800, "finalLoss": 1.2})
            self.assertEqual((out / "checkpoints" / "LATEST").read_text(encoding="utf-8").strip(), "step-8")
            self.assertFalse((out / "checkpoints" / "step-4").exists(), "the previous checkpoint is dropped")
            state = json.loads((out / "checkpoints" / "step-8" / "state.json").read_text(encoding="utf-8"))
            self.assertEqual((state["step"], state["nextStart"]), (8, 24))
            self.assertEqual(adapter.saved, ["step-4", "step-8"])

    def test_the_admitted_window_only_ever_sheds(self):
        """The window is the lever of last resort once micro-batching floors at one.

        Measured on the 5090 (2026-09-25): a 27B QLoRA plan came to 30.80 GiB of a
        31.84 GiB card with microBatchSize already 1 — the corrected logits term
        exposed that truth rather than causing it. Shedding the window is what keeps
        such a job runnable, and it must never shed below what still teaches nor grow
        past what was asked for.
        """
        floor = cuda_train.MINIMUM_TRAIN_TOKENS
        # Fits: the requested window stands, untouched.
        self.assertEqual(cuda_train.admitted_window(3348, 3348, 10_000), 3348)
        self.assertEqual(cuda_train.admitted_window(3348, 3348, 3348), 3348)
        # Does not fit: shed to what the budget affords.
        self.assertEqual(cuda_train.admitted_window(3348, 3348, 2000), 2000)
        # Below what still carries a write-error-fix trajectory there is NO window: the
        # reviewer's point on #4396 — shedding to a 512 floor the budget cannot hold is
        # an OOM or a wait that never ends. The plan reports unaffordable instead.
        self.assertIsNone(cuda_train.admitted_window(3348, 3348, 8))
        self.assertIsNone(cuda_train.admitted_window(3348, 3348, -1),
                          "a budget smaller than the weights is a refusal with numbers, not a negative window")
        self.assertIsNone(cuda_train.admitted_window(3348, 3348, floor - 1))
        self.assertEqual(cuda_train.admitted_window(3348, 3348, floor), floor)
        # Never ABOVE the requested window: a 32-token fixture that fits reports 32, not
        # the 512 floor; one that does not fit is unaffordable like any other.
        self.assertEqual(cuda_train.admitted_window(32, 32, 32), 32)
        self.assertEqual(cuda_train.admitted_window(32, 32, 10_000), 32)
        self.assertIsNone(cuda_train.admitted_window(32, 32, 0))

    def test_the_window_is_decided_against_grantable_not_free_now(self):
        """The 5090 tonight (Cormac + Fable on #4396, 2026-09-26): her serving lane up,
        the governor exposes ~2.15 GB free-now and ~31.7 GB grantable (serving's lane
        added back). The 27B QLoRA plan is 28.48 GB at the full 3348-token window.
        Against free-now the window is unaffordable and every dispatch while she
        serves is refused; against grantable it is the full window and the job PARKS
        for the period, which is the held queue the run relies on."""
        gib = 1024 ** 3
        total, free_now, grantable = int(31.84 * gib), int(2.15 * 1e9), int(31.7 * 1e9)
        slab = 20 * 1024 * 1024
        # Qwen3.8-27B-class geometry: 64 layers, hidden 5120, vocab 151936; 24.15 GB of
        # weights at nf4-double, ~0.5 GB of optimizer state for the LoRA.
        weights, optimizer = int(24.15e9), int(0.5e9)
        activation_per_token = 5120 * (64 + 1) * 4
        logits_chunk = min(3348, cuda_train.LOGITS_CHUNK_TOKENS) * 151936 * cuda_train.LOGITS_BYTES_PER_TOKEN
        against_free = cuda_train.affordable_tokens(cuda_train.planning_budget(total, free_now),
                                                    weights, optimizer, logits_chunk, activation_per_token, slab)
        self.assertLess(against_free, 0, "free-now is smaller than the weights: a refusal, if it were the budget")
        self.assertIsNone(cuda_train.admitted_window(3348, 3348, against_free))
        against_grantable = cuda_train.affordable_tokens(cuda_train.planning_budget(total, grantable),
                                                         weights, optimizer, logits_chunk, activation_per_token, slab)
        self.assertGreaterEqual(against_grantable, 3348, f"the card empty holds the whole window ({against_grantable})")
        self.assertEqual(cuda_train.admitted_window(3348, 3348, against_grantable), 3348)
        # The planning budget never exceeds the card, whatever the governor says.
        self.assertEqual(cuda_train.planning_budget(total, 10 * total), total)
        self.assertEqual(cuda_train.planning_budget(total, -5), 0)

    @unittest.skipUnless(os.environ.get("CONTINUUM_TEST_CUDA") == "1", "real CUDA test opt-in")
    def test_real_qlora_trains_adapter_without_changing_base(self):
        import torch
        from transformers import Qwen2Config, Qwen2Tokenizer, Qwen3_5Config, Qwen3_5TextConfig, Qwen3_5VisionConfig
        from tokenizers.pre_tokenizers import ByteLevel
        from safetensors.torch import load_file
        self.assertTrue(torch.cuda.is_available())
        for family in ["qwen2", "qwen3_5"]:
            with self.subTest(family=family):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    base, output = root / "base", root / "adapter"
                    torch.manual_seed(7)
                    vocab = {char: index for index, char in enumerate(sorted(ByteLevel.alphabet()))}
                    vocab["<|endoftext|>"] = len(vocab)
                    if family == "qwen2":
                        config = Qwen2Config(vocab_size=len(vocab), hidden_size=64,
                            intermediate_size=128, num_hidden_layers=2, num_attention_heads=4,
                            num_key_value_heads=2, max_position_embeddings=128)
                    else:
                        # Kimi's actual model family: hybrid linear/full attention and
                        # a multimodal wrapper, scaled only for a kernel regression.
                        config = Qwen3_5Config(
                            text_config=Qwen3_5TextConfig(vocab_size=len(vocab), hidden_size=64,
                                intermediate_size=128, num_hidden_layers=4, num_attention_heads=4,
                                num_key_value_heads=2, head_dim=16, linear_key_head_dim=16,
                                linear_value_head_dim=16, linear_num_key_heads=2,
                                linear_num_value_heads=4, max_position_embeddings=128,
                                mtp_num_hidden_layers=1,
                                layer_types=["linear_attention"] * 3 + ["full_attention"],
                                rope_parameters={"rope_type":"default", "rope_theta":10000,
                                    "partial_rotary_factor":1.0, "mrope_interleaved":True,
                                    "mrope_section":[2,3,3]}),
                            vision_config=Qwen3_5VisionConfig(depth=1, hidden_size=32,
                                intermediate_size=64, num_heads=4, out_hidden_size=64,
                                patch_size=2, temporal_patch_size=1))
                    model = cuda_train.model_class(config).from_config(config)
                    if family == "qwen3_5":
                        self.assertEqual(type(model).__name__, "Qwen3_5ForConditionalGeneration")
                    model.save_pretrained(base)
                    del model
                    tokenizer = Qwen2Tokenizer(vocab=vocab, merges=[])
                    # Regression: completed training messages and generation prompts
                    # differ on thinking models; generation framing is not a loss mask.
                    tokenizer.chat_template = ("{% for m in messages %}{{m['role']}}: "
                        "{% if m['role']=='assistant' %}<think></think>{% endif %}"
                        "{{m['content']}}\n{% endfor %}"
                        "{% if add_generation_prompt %}assistant: <think>{% endif %}")
                    tokenizer.save_pretrained(base)
                    original = hashlib.sha256((base / "model.safetensors").read_bytes()).digest()
                    spec = {"baseModel": str(base), "dataset": {"examples": [
                        {"prompt":"old state", "completion":"read current receipt"},
                        {"prompt":"new receipt", "completion":"read current state"},
                        {"prompt":"old receipt", "completion":"read new state"},
                        {"prompt":"current receipt", "completion":"read current state"}], "validationSplit":0.25},
                        "schedule":{"epochs":2, "batchSize":1, "sequenceLength":32, "learningRate":0.001},
                        "lora":{"rank":4,"alpha":8,"dropout":0.0,"targetModules":["q_proj","v_proj"]}}
                    cuda_train.plan(spec, root / "plan.json")
                    planned = json.loads((root / "plan.json").read_text())
                    # No free bytes is a queueable plan, not a failed trainer.
                    spec["availableBytes"] = 0
                    cuda_train.plan(spec, root / "waiting-plan.json")
                    waiting = json.loads((root / "waiting-plan.json").read_text())
                    self.assertEqual(waiting["microBatchSize"], 1)
                    self.assertGreater(waiting["memoryBytes"], 0)
                    self.assertEqual(waiting["memoryBytes"], sum(waiting["terms"].values()))
                    # This is a mechanics fixture, not Kimi learning or a model benchmark.
                    spec["schedule"]["batchSize"] = 3
                    spec["availableBytes"] = planned["memoryBytes"]
                    cuda_train.plan(spec, root / "plan.json")
                    planned = json.loads((root / "plan.json").read_text())
                    spec.update(memoryBytes=planned["memoryBytes"], revision=planned["revision"],
                                microBatchSize=1)  # Force accumulation within the admitted footprint.
                    cuda_train.train(spec, output)
                    adapter_config = json.loads((output / "adapter_config.json").read_text())
                    self.assertEqual(adapter_config["base_model_name_or_path"], spec["baseModel"])
                    receipt = json.loads((output / "training-provenance.json").read_text())
                    self.assertEqual(receipt["effectiveBatchSize"], 3)
                    self.assertEqual(receipt["microBatchSize"], 1)
                    self.assertEqual(receipt["steps"], 2 * math.ceil(receipt["trainingRows"] / 3))
                    weights = load_file(output / "adapter_model.safetensors")
                    self.assertTrue(any(torch.count_nonzero(v).item() > 0 for k,v in weights.items() if "lora_B" in k))
                    metrics = json.loads((output / "metrics.json").read_text())
                    self.assertGreater(metrics["trainedTokens"], 0)
                    self.assertIsNotNone(metrics["finalValidationLoss"])
                    self.assertEqual(hashlib.sha256((base / "model.safetensors").read_bytes()).digest(), original)
                    if os.environ.get("CONTINUUM_TEST_GGUF") == "1":
                        # Same pinned upstream converter used by forge-custodian.
                        converter = Path(__file__).resolve().parents[4] / "vendor/llama.cpp/convert_lora_to_gguf.py"
                        gene = root / "gene.gguf"
                        converted = subprocess.run([sys.executable, str(converter), str(output),
                            "--base", str(base), "--outtype", "f16", "--outfile", str(gene)],
                            capture_output=True, text=True, encoding="utf-8")
                        self.assertEqual(converted.returncode, 0, converted.stderr)
                        sys.path.insert(0, str(converter.parent / "gguf-py"))
                        import gguf
                        reader = gguf.GGUFReader(str(gene))
                        self.assertEqual(len(reader.tensors), len(weights))
                        self.assertTrue(all(".lora_" in tensor.name for tensor in reader.tensors))
                        del reader  # Release the Windows mapping before tempdir cleanup.




if __name__ == "__main__":
    unittest.main()
