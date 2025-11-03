#!/usr/bin/env python
"""
Minimal Sentinel-AI Bridge for Continuum
IMMEDIATE USE - loads Sentinel models for PersonaUsers
"""

import sys
import json
from pathlib import Path

# Add Sentinel-AI to path
SENTINEL_PATH = Path('/Volumes/FlashGordon/cambrian/sentinel-ai')
if SENTINEL_PATH.exists():
    sys.path.insert(0, str(SENTINEL_PATH))
else:
    print(json.dumps({"error": f"Sentinel-AI not found at {SENTINEL_PATH}"}))
    sys.exit(1)

try:
    from sentinel.models.adaptive_transformer import AdaptiveCausalLmWrapper
    # Note: Pruning functionality requires additional Sentinel modules
    # For now, focus on basic loading and generation
    try:
        from sentinel.models.agency_specialization import AgencySpecialization
        AGENCY_AVAILABLE = True
    except ImportError:
        AGENCY_AVAILABLE = False
except ImportError as e:
    print(json.dumps({"error": f"Failed to import Sentinel: {e}"}))
    sys.exit(1)


class SentinelBridge:
    """Minimal bridge to load and use Sentinel models in Continuum."""

    def __init__(self):
        self.model = None
        self.agency = None

    def load(self, model_name='distilgpt2', device='mps', pruning_level=0.0):
        """Load Sentinel model."""
        import torch

        device_obj = torch.device(device if torch.backends.mps.is_available() and device == 'mps' else 'cpu')

        self.model = AdaptiveCausalLmWrapper(model_name, device=device_obj)

        if AGENCY_AVAILABLE:
            self.agency = AgencySpecialization(self.model)

        # Note: Pruning not yet implemented in bridge
        # See Sentinel-AI repo for pruning experiments

        return {
            "success": True,
            "model_name": model_name,
            "device": str(device_obj),
            "num_layers": self.model.num_layers,
            "num_heads": self.model.num_heads,
            "total_heads": self.model.num_layers * self.model.num_heads,
            "agency_available": AGENCY_AVAILABLE,
            "pruning_level": pruning_level if pruning_level == 0 else "not_implemented"
        }

    def generate(self, prompt, max_length=50):
        """Generate text using Sentinel model."""
        if not self.model:
            return {"error": "Model not loaded"}

        from transformers import AutoTokenizer
        import torch

        tokenizer = AutoTokenizer.from_pretrained(self.model.model_name)
        inputs = tokenizer(prompt, return_tensors="pt").to(self.model.device)

        with torch.no_grad():
            outputs = self.model.generate(
                **inputs,
                max_length=max_length,
                do_sample=True,
                temperature=0.7,
                top_p=0.9
            )

        text = tokenizer.decode(outputs[0], skip_special_tokens=True)

        return {
            "success": True,
            "prompt": prompt,
            "generated": text
        }

    def get_agency_status(self):
        """Get current agency signals from model heads."""
        if not self.model:
            return {"error": "Model not loaded"}

        agency_data = []
        for layer_idx in range(self.model.num_layers):
            block = self.model.blocks[layer_idx]
            attn = block["attn"]

            if hasattr(attn, 'agency_signals'):
                for head_idx, signal in attn.agency_signals.items():
                    agency_data.append({
                        "layer": layer_idx,
                        "head": head_idx,
                        "state": signal.get("state", "unknown"),
                        "consent": signal.get("consent", True),
                        "utilization": signal.get("utilization", 0.0)
                    })

        return {
            "success": True,
            "agency_signals": agency_data
        }


def main():
    """CLI interface for Continuum to call."""
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument('--action', required=True,
                       choices=['load', 'generate', 'agency-status'])
    parser.add_argument('--model', default='distilgpt2')
    parser.add_argument('--device', default='mps')
    parser.add_argument('--pruning-level', type=float, default=0.0)
    parser.add_argument('--prompt', default='')
    parser.add_argument('--max-length', type=int, default=50)

    args = parser.parse_args()

    bridge = SentinelBridge()

    try:
        if args.action == 'load':
            result = bridge.load(args.model, args.device, args.pruning_level)
        elif args.action == 'generate':
            # Load first if not loaded
            if not bridge.model:
                bridge.load(args.model, args.device, args.pruning_level)
            result = bridge.generate(args.prompt, args.max_length)
        elif args.action == 'agency-status':
            if not bridge.model:
                result = {"error": "Model not loaded - call 'load' first"}
            else:
                result = bridge.get_agency_status()
        else:
            result = {"error": f"Unknown action: {args.action}"}

        print(json.dumps(result, indent=2))

    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == '__main__':
    main()
