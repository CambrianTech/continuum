"""
Adapter Conversion Script — Bidirectional LoRA ↔ GGUF Conversion

Operations:
  merge-full       Merge LoRA into FP16 base model (for full-precision deployment)
  merge-and-quantize  Merge LoRA into base → quantize to GGUF (single shipping file)
  quantize-base    Convert HF safetensors model to GGUF (no LoRA involved)
  validate         Load converted model and run sanity inference

Key insight: QLoRA training produces FP16 LoRA weights. The adapter works on both
quantized and non-quantized base models. Conversion is about the BASE MODEL format,
not the adapter format.

Usage:
    python convert-adapter.py merge-full --adapter /path/to/adapter --base model_name --output /path/out
    python convert-adapter.py merge-and-quantize --adapter /path/to/adapter --base model_name --bits 4 --output /path/out
    python convert-adapter.py quantize-base --base model_name --bits 4 --output /path/out
    python convert-adapter.py validate --model /path/to/model --prompt "Hello"

Requirements:
    torch, transformers, peft, auto-gptq (optional), llama-cpp-python (optional)
"""

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Dict, Any, Optional

# Check dependencies
try:
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer
    from peft import PeftModel
    print("✅ Core libraries imported (torch, transformers, peft)")
except ImportError as e:
    print(f"❌ Missing dependency: {e}")
    print("   Install with: pip install torch transformers peft")
    sys.exit(1)


def merge_lora_full(adapter_path: str, base_model: str, output_path: str) -> Dict[str, Any]:
    """Merge LoRA adapter into base model and save as FP16 safetensors.

    This produces a standalone model that doesn't need the adapter at runtime.
    Use case: deploy to 5090 for full-precision inference.
    """
    print(f"\n🔀 Merging LoRA into full-precision model")
    print(f"   Adapter: {adapter_path}")
    print(f"   Base model: {base_model}")
    print(f"   Output: {output_path}")
    start = time.time()

    # Load base model in FP16
    print("   Loading base model (FP16)...")
    model = AutoModelForCausalLM.from_pretrained(
        base_model,
        torch_dtype=torch.float16,
        device_map="cpu",  # Merge on CPU to avoid VRAM pressure
        trust_remote_code=True,
        low_cpu_mem_usage=True,
    )
    tokenizer = AutoTokenizer.from_pretrained(base_model, trust_remote_code=True)

    # Load and merge LoRA adapter
    print("   Loading LoRA adapter...")
    model = PeftModel.from_pretrained(model, adapter_path)

    print("   Merging weights...")
    model = model.merge_and_unload()

    # Save merged model
    print(f"   Saving to {output_path}...")
    os.makedirs(output_path, exist_ok=True)
    model.save_pretrained(output_path, safe_serialization=True)
    tokenizer.save_pretrained(output_path)

    duration = time.time() - start
    size_mb = sum(f.stat().st_size for f in Path(output_path).rglob("*") if f.is_file()) / (1024 * 1024)

    result = {
        "operation": "merge-full",
        "outputPath": output_path,
        "format": "safetensors-fp16",
        "sizeMB": round(size_mb, 2),
        "durationSeconds": round(duration, 2),
        "baseModel": base_model,
        "adapterPath": adapter_path,
    }

    print(f"✅ Merge complete: {size_mb:.1f}MB in {duration:.1f}s")
    return result


def merge_and_quantize(adapter_path: str, base_model: str, output_path: str, bits: int = 4) -> Dict[str, Any]:
    """Merge LoRA into base model, then quantize to GGUF.

    Two-step process:
      1. Merge LoRA → full-precision model (like merge-full)
      2. Convert merged model to GGUF quantized format

    Use case: ship a single GGUF file for deployment on M1/resource-constrained hardware.
    """
    print(f"\n🔀 Merge + Quantize to GGUF ({bits}-bit)")
    print(f"   Adapter: {adapter_path}")
    print(f"   Base model: {base_model}")
    print(f"   Output: {output_path}")
    start = time.time()

    # Step 1: Merge into temp directory
    temp_merged = output_path + "-merged-temp"
    merge_result = merge_lora_full(adapter_path, base_model, temp_merged)

    # Step 2: Quantize to GGUF
    print(f"\n   Quantizing to GGUF Q{bits}_0...")

    # Try llama-cpp-python's convert tool
    gguf_path = os.path.join(output_path, f"model-q{bits}_0.gguf")
    os.makedirs(output_path, exist_ok=True)

    try:
        from llama_cpp import llama_model_quantize
        # Use llama.cpp's built-in quantization
        print("   Using llama-cpp-python for quantization...")

        # First convert to GGUF format using the convert script
        import subprocess
        convert_script = _find_convert_script()
        if convert_script:
            # Convert HF format to GGUF
            fp16_gguf = os.path.join(output_path, "model-f16.gguf")
            subprocess.run(
                [sys.executable, str(convert_script), temp_merged, "--outfile", fp16_gguf, "--outtype", "f16"],
                check=True, capture_output=True, text=True,
            )
            print(f"   F16 GGUF created: {fp16_gguf}")

            # Quantize
            quant_type = {4: 2, 8: 7}  # Q4_0=2, Q8_0=7 in llama.cpp
            llama_model_quantize(fp16_gguf, gguf_path, quant_type.get(bits, 2))

            # Clean up F16 intermediate
            os.remove(fp16_gguf)
            print(f"   Quantized to: {gguf_path}")
        else:
            raise ImportError("llama.cpp convert script not found")

    except (ImportError, Exception) as e:
        print(f"   ⚠️ llama-cpp-python quantization failed: {e}")
        print(f"   Falling back to manual conversion...")

        # Fallback: try using transformers + gguf export
        try:
            import subprocess
            convert_script = _find_convert_script()
            if convert_script:
                quant_type = f"q{bits}_0"
                subprocess.run(
                    [sys.executable, str(convert_script), temp_merged,
                     "--outfile", gguf_path, "--outtype", quant_type],
                    check=True, capture_output=True, text=True,
                )
                print(f"   Converted with llama.cpp convert script")
            else:
                # Last resort: save as safetensors (not GGUF, but usable)
                print(f"   ⚠️ No GGUF converter available, saving as safetensors instead")
                gguf_path = output_path  # Just use the merged output
                # The merged temp IS the output
                import shutil
                if os.path.exists(output_path):
                    shutil.rmtree(output_path)
                os.rename(temp_merged, output_path)
                temp_merged = None  # Don't clean up
        except Exception as e2:
            print(f"   ❌ Fallback also failed: {e2}")
            raise

    # Clean up temp merged model
    if temp_merged and os.path.exists(temp_merged):
        import shutil
        shutil.rmtree(temp_merged)

    duration = time.time() - start
    size_mb = os.path.getsize(gguf_path) / (1024 * 1024) if os.path.isfile(gguf_path) else merge_result["sizeMB"]

    result = {
        "operation": "merge-and-quantize",
        "outputPath": gguf_path if os.path.isfile(gguf_path) else output_path,
        "format": f"gguf-q{bits}_0",
        "sizeMB": round(size_mb, 2),
        "durationSeconds": round(duration, 2),
        "baseModel": base_model,
        "adapterPath": adapter_path,
        "bits": bits,
        "compressionRatio": round(merge_result["sizeMB"] / max(size_mb, 0.01), 2),
    }

    print(f"✅ Merge + quantize complete: {size_mb:.1f}MB in {duration:.1f}s")
    return result


def quantize_base(base_model: str, output_path: str, bits: int = 4) -> Dict[str, Any]:
    """Quantize a HuggingFace model to GGUF format (no LoRA involved).

    Use case: prepare a base model for local quantized inference.
    """
    print(f"\n📦 Quantizing base model to GGUF ({bits}-bit)")
    print(f"   Base model: {base_model}")
    print(f"   Output: {output_path}")
    start = time.time()

    os.makedirs(output_path, exist_ok=True)
    gguf_path = os.path.join(output_path, f"model-q{bits}_0.gguf")

    # Download model if needed
    print("   Downloading/loading model...")
    from huggingface_hub import snapshot_download
    model_dir = snapshot_download(base_model)

    # Convert using llama.cpp's convert script
    convert_script = _find_convert_script()
    if not convert_script:
        raise RuntimeError(
            "llama.cpp convert script not found. Install llama-cpp-python or "
            "clone llama.cpp and set LLAMA_CPP_DIR environment variable."
        )

    import subprocess
    quant_type = f"q{bits}_0"
    subprocess.run(
        [sys.executable, str(convert_script), model_dir,
         "--outfile", gguf_path, "--outtype", quant_type],
        check=True, capture_output=True, text=True,
    )

    duration = time.time() - start
    size_mb = os.path.getsize(gguf_path) / (1024 * 1024)

    result = {
        "operation": "quantize-base",
        "outputPath": gguf_path,
        "format": f"gguf-q{bits}_0",
        "sizeMB": round(size_mb, 2),
        "durationSeconds": round(duration, 2),
        "baseModel": base_model,
        "bits": bits,
    }

    print(f"✅ Quantization complete: {size_mb:.1f}MB in {duration:.1f}s")
    return result


def validate_model(model_path: str, prompt: str = "Hello, how are you?") -> Dict[str, Any]:
    """Load a converted model and run sanity inference.

    Verifies the conversion didn't corrupt the model by:
      1. Loading the model
      2. Running a simple prompt
      3. Checking output isn't garbage (no replacement chars, reasonable length)
    """
    print(f"\n🔍 Validating model at: {model_path}")
    print(f"   Prompt: {prompt}")
    start = time.time()

    is_gguf = model_path.endswith(".gguf")

    if is_gguf:
        # Validate GGUF via llama-cpp-python
        try:
            from llama_cpp import Llama
            model = Llama(model_path=model_path, n_ctx=512, n_gpu_layers=-1, verbose=False)
            output = model(prompt, max_tokens=50, temperature=0.7)
            text = output["choices"][0]["text"]
        except ImportError:
            print("   ⚠️ llama-cpp-python not installed, skipping GGUF validation")
            return {"valid": False, "error": "llama-cpp-python not installed"}
    else:
        # Validate safetensors/HF model
        tokenizer = AutoTokenizer.from_pretrained(model_path, trust_remote_code=True)
        model = AutoModelForCausalLM.from_pretrained(
            model_path, torch_dtype=torch.float16, device_map="auto", trust_remote_code=True,
        )
        inputs = tokenizer(prompt, return_tensors="pt").to(model.device)
        with torch.no_grad():
            outputs = model.generate(**inputs, max_new_tokens=50, temperature=0.7, do_sample=True)
        text = tokenizer.decode(outputs[0][inputs["input_ids"].shape[1]:], skip_special_tokens=True)

    duration = time.time() - start

    # Sanity checks
    has_garbage = '\ufffd' in text  # Unicode replacement character
    is_empty = len(text.strip()) == 0
    is_repetitive = len(set(text.split())) < 3 and len(text) > 20

    valid = not has_garbage and not is_empty and not is_repetitive

    result = {
        "valid": valid,
        "outputText": text[:200],
        "outputTokens": len(text.split()),
        "durationSeconds": round(duration, 2),
        "modelPath": model_path,
        "checks": {
            "noGarbage": not has_garbage,
            "notEmpty": not is_empty,
            "notRepetitive": not is_repetitive,
        }
    }

    if valid:
        print(f"✅ Validation passed: \"{text[:100]}...\"")
    else:
        print(f"❌ Validation FAILED")
        if has_garbage: print(f"   - Contains garbage characters")
        if is_empty: print(f"   - Output is empty")
        if is_repetitive: print(f"   - Output is repetitive")

    return result


def _find_convert_script() -> Optional[Path]:
    """Find llama.cpp's convert-hf-to-gguf.py script."""
    # Check LLAMA_CPP_DIR environment variable
    llama_dir = os.environ.get("LLAMA_CPP_DIR")
    if llama_dir:
        script = Path(llama_dir) / "convert_hf_to_gguf.py"
        if script.exists():
            return script
        # Try older name
        script = Path(llama_dir) / "convert-hf-to-gguf.py"
        if script.exists():
            return script

    # Check common locations
    candidates = [
        Path.home() / "llama.cpp" / "convert_hf_to_gguf.py",
        Path.home() / "llama.cpp" / "convert-hf-to-gguf.py",
        Path("/opt/llama.cpp/convert_hf_to_gguf.py"),
        Path("/usr/local/share/llama.cpp/convert_hf_to_gguf.py"),
    ]

    # Also check if llama-cpp-python installed it somewhere
    try:
        import llama_cpp
        pkg_dir = Path(llama_cpp.__file__).parent
        script = pkg_dir / "convert_hf_to_gguf.py"
        if script.exists():
            return script
    except ImportError:
        pass

    for candidate in candidates:
        if candidate.exists():
            return candidate

    return None


def main():
    parser = argparse.ArgumentParser(description="Adapter Conversion Tool")
    subparsers = parser.add_subparsers(dest="operation", required=True)

    # merge-full
    merge_full_parser = subparsers.add_parser("merge-full", help="Merge LoRA into FP16 base model")
    merge_full_parser.add_argument("--adapter", required=True, help="Path to LoRA adapter directory")
    merge_full_parser.add_argument("--base", required=True, help="Base model name or path")
    merge_full_parser.add_argument("--output", required=True, help="Output directory")

    # merge-and-quantize
    mq_parser = subparsers.add_parser("merge-and-quantize", help="Merge LoRA + quantize to GGUF")
    mq_parser.add_argument("--adapter", required=True, help="Path to LoRA adapter directory")
    mq_parser.add_argument("--base", required=True, help="Base model name or path")
    mq_parser.add_argument("--bits", type=int, default=4, choices=[4, 8], help="Quantization bits")
    mq_parser.add_argument("--output", required=True, help="Output directory")

    # quantize-base
    qb_parser = subparsers.add_parser("quantize-base", help="Quantize HF model to GGUF")
    qb_parser.add_argument("--base", required=True, help="Base model name or path")
    qb_parser.add_argument("--bits", type=int, default=4, choices=[4, 8], help="Quantization bits")
    qb_parser.add_argument("--output", required=True, help="Output directory")

    # validate
    val_parser = subparsers.add_parser("validate", help="Validate a converted model")
    val_parser.add_argument("--model", required=True, help="Path to model (directory or GGUF file)")
    val_parser.add_argument("--prompt", default="Hello, how are you?", help="Test prompt")

    args = parser.parse_args()

    print("🧬 Adapter Conversion Tool")
    print("=" * 60)

    if args.operation == "merge-full":
        result = merge_lora_full(args.adapter, args.base, args.output)
    elif args.operation == "merge-and-quantize":
        result = merge_and_quantize(args.adapter, args.base, args.output, args.bits)
    elif args.operation == "quantize-base":
        result = quantize_base(args.base, args.output, args.bits)
    elif args.operation == "validate":
        result = validate_model(args.model, args.prompt)

    # Write result JSON for the calling process to read
    result_path = os.path.join(
        args.output if hasattr(args, 'output') else os.path.dirname(args.model),
        "conversion_result.json"
    )
    with open(result_path, 'w') as f:
        json.dump(result, f, indent=2)
    print(f"\n📊 Result written to: {result_path}")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"\n❌ Conversion failed: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
