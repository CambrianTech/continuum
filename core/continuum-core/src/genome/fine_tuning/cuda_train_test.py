"""Contract tests, plus an explicitly opted-in real CUDA/PEFT kernel test."""
import hashlib
import json
import math
import os
from pathlib import Path
import tempfile
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

    @unittest.skipUnless(os.environ.get("CONTINUUM_TEST_CUDA") == "1", "real CUDA test opt-in")
    def test_real_qlora_trains_adapter_without_changing_base(self):
        import torch
        from transformers import Qwen2Config, Qwen2ForCausalLM, Qwen2Tokenizer
        from tokenizers.pre_tokenizers import ByteLevel
        from safetensors.torch import load_file
        self.assertTrue(torch.cuda.is_available())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base, output = root / "base", root / "adapter"
            torch.manual_seed(7)
            vocab = {char: index for index, char in enumerate(sorted(ByteLevel.alphabet()))}
            vocab["<|endoftext|>"] = len(vocab)
            Qwen2ForCausalLM(Qwen2Config(vocab_size=len(vocab), hidden_size=64,
                intermediate_size=128, num_hidden_layers=2, num_attention_heads=4,
                num_key_value_heads=2, max_position_embeddings=128)).save_pretrained(base)
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
            # This is a mechanics fixture, not Kimi learning or a model benchmark.
            spec["schedule"]["batchSize"] = 3
            spec["availableBytes"] = planned["memoryBytes"]
            cuda_train.plan(spec, root / "plan.json")
            planned = json.loads((root / "plan.json").read_text())
            spec.update(memoryBytes=planned["memoryBytes"], revision=planned["revision"],
                        microBatchSize=1)  # Force accumulation within the admitted footprint.
            cuda_train.train(spec, output)
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


if __name__ == "__main__":
    unittest.main()
