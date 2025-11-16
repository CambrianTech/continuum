# PEFT Dynamic Composition - Python Environment

**Purpose**: Local PEFT-based LoRA composition and inference (Tier 1 & Tier 3)

**Separate from training environment**: This is NOT for training - that uses `.continuum/genome/python/` (Conda/micromamba). This environment is for **composing trained adapters** and running inference with multiple LoRA layers.

---

## Setup

```bash
cd system/genome/python

# Create virtual environment
python3 -m venv venv

# Activate
source venv/bin/activate

# Install dependencies
pip install --upgrade pip
pip install -r requirements.txt
```

---

## Files

- **`requirements.txt`** - PEFT, transformers, torch for composition
- **`peft_composition.py`** - Dynamic LoRA composition prototype
- **`download_openai_adapter.py`** - Download adapter metadata from OpenAI
- **`venv/`** - Virtual environment (gitignored)

---

## Relationship with Training Environment

**Two environments, two purposes:**

| Environment | Location | Purpose | Used By |
|-------------|----------|---------|---------|
| **Training** | `.continuum/genome/python/` | API-based fine-tuning (Sentinel bridge) | Tier 2 (cloud training) |
| **PEFT Composition** | `system/genome/python/` | Local adapter composition & inference | Tier 1 (demo) & Tier 3 (advanced) |

**Can run simultaneously:**
- Training env handles subprocess calls to Sentinel/provider APIs
- PEFT env handles local inference with composed adapters
- Different Python processes, no conflict

---

## Usage

### Compose Adapters
```python
from peft_composition import PEFTComposer

# Initialize with base model
composer = PEFTComposer("meta-llama/Llama-3.2-1B")

# Load multiple adapters
composer.load_adapter("./adapters/wine-expertise", "wine")
composer.load_adapter("./adapters/vin-diesel-style", "personality")

# Set composition (instant switching!)
composer.set_composition(["wine", "personality"], [0.7, 0.3])

# Generate with both adapters active
response = composer.generate("Tell me about Cabernet Sauvignon")
```

### Download Adapter Metadata
```python
from download_openai_adapter import download_openai_adapter

# OpenAI adapters are API-only (no weights), but save metadata
download_openai_adapter(
    model_id="ft:gpt-4o-mini-2024-07-18:personal::CcKeiPN2",
    output_dir="./adapters/openai-wine-expert"
)
```

---

## Target Hardware

**Tier 1 (M1 MacBook - 32GB):**
- Llama 3.2 1B (4-bit quantized): ~1.5GB
- 2-3 LoRA adapters: ~300MB
- **maxActiveLayers: 3** (real multi-layer genome!)

**Tier 3 (High-End GPU):**
- Llama 3.1 8B/70B with multi-layer composition
- Unlimited adapters (memory permitting)
- TIES/DARE advanced merging

---

## Next Steps

1. **TypeScript wrapper** - Call Python from Node.js via subprocess
2. **Adapter download scripts** - Fireworks, Together (downloadable weights)
3. **GenomeLayerEntity integration** - Store adapters in database
4. **PersonaGenome class** - Route to PEFT for local inference

See: `docs/genome/PROVIDER-CAPABILITIES-SUMMARY.md` for full architecture
