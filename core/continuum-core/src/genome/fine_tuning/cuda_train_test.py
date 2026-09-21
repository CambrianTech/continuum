"""Contract tests, plus an explicitly opted-in real CUDA/PEFT kernel test."""
import hashlib
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
