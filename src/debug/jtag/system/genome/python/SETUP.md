# PEFT Environment Setup for New Users

## Quick Start (Automatic)

When you first try to use PEFT composition, the system will automatically bootstrap the environment:

```bash
# Just run this - it auto-detects missing environment and sets up
./jtag genome/compose --adapters wine-expertise,vin-diesel-style

# On first run, you'll see:
# "📦 PEFT environment not found, creating..."
# [automatic setup runs]
# "✅ PEFT environment ready"
```

## Manual Setup (If Needed)

If you want to set up the environment manually:

```bash
cd system/genome/python

# Create virtual environment
python3 -m venv venv

# Activate
source venv/bin/activate  # macOS/Linux
# OR
venv\Scripts\activate  # Windows

# Install dependencies
pip install --upgrade pip
pip install -r requirements.txt
```

**Time**: ~2-3 minutes (downloads PyTorch, PEFT, transformers)

**Disk space**: ~3-4GB (PyTorch wheels)

## Requirements

**Python**: 3.9+ (3.11 recommended)

**Hardware (depends on tier):**
- **Tier 1 (Demo)**: M1/M2 Mac, or any system with 8GB+ RAM
- **Tier 2 (Cloud APIs)**: Any system with internet (no local GPU needed)
- **Tier 3 (Advanced)**: 24GB+ VRAM GPU (RTX 3090/4090 or better)

## Verification

Check if environment is working:

```bash
cd system/genome/python
source venv/bin/activate
python peft_composition.py --help

# Should show usage instructions (not errors)
```

## Troubleshooting

### Issue: "ModuleNotFoundError: No module named 'torch'"

**Fix**: Activate the virtual environment first
```bash
source venv/bin/activate  # You forgot this step!
pip install -r requirements.txt
```

### Issue: "Python version too old"

**Fix**: Upgrade Python or use conda
```bash
# Check version
python3 --version  # Need 3.9+

# Option 1: Use system Python 3.11+
python3.11 -m venv venv

# Option 2: Use conda instead
conda create -n peft-composition python=3.11 -y
conda activate peft-composition
pip install -r requirements.txt
```

### Issue: "Disk space full during pip install"

**Fix**: PyTorch is large (~3GB), free up space
```bash
# Check available space
df -h

# Clean up if needed
pip cache purge
```

### Issue: "ImportError: cannot import name 'PeftModel'"

**Fix**: PEFT version too old
```bash
pip install --upgrade peft transformers
```

## CI/CD Integration

For automated testing/deployment:

```bash
# .github/workflows/test.yml (example)
- name: Setup PEFT Environment
  run: |
    cd src/debug/jtag/system/genome/python
    python3 -m venv venv
    source venv/bin/activate
    pip install -r requirements.txt

- name: Test PEFT Composition
  run: |
    source system/genome/python/venv/bin/activate
    python system/genome/python/peft_composition.py --help
```

## Docker Support

For containerized deployment:

```dockerfile
# Dockerfile (example)
FROM python:3.11-slim

WORKDIR /app

# Copy PEFT environment
COPY system/genome/python/requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

# Your app code
COPY . .

CMD ["python", "system/genome/python/peft_composition.py"]
```

## Why Separate from Training Environment?

**Two Python environments serve different purposes:**

| Environment | Location | Purpose |
|-------------|----------|---------|
| **Training** | `.continuum/genome/python/` | Cloud API training (Sentinel bridge) |
| **PEFT** | `system/genome/python/` | Local adapter composition & inference |

**Key difference:**
- Training env: Heavy dependencies (Sentinel SDK), used for submitting jobs
- PEFT env: Lightweight, just inference (PEFT + transformers)

**Both can run simultaneously** - different processes, no conflicts

## Platform-Specific Notes

### macOS (M1/M2)
```bash
# Use native ARM builds (faster)
pip install torch torchvision --index-url https://download.pytorch.org/whl/cpu

# Metal GPU support (optional, for M1/M2)
pip install torch torchvision --index-url https://download.pytorch.org/whl/cpu
```

### Linux (CUDA)
```bash
# Install CUDA-enabled PyTorch
pip install torch torchvision --index-url https://download.pytorch.org/whl/cu118
```

### Windows
```bash
# Standard installation works
python -m venv venv
venv\Scripts\activate
pip install -r requirements.txt
```

## Next Steps

After setup, see:
- `README.md` - Usage examples
- `docs/genome/PROVIDER-CAPABILITIES-SUMMARY.md` - Full architecture
- `peft_composition.py` - Composition prototype

**Questions?** Check the main genome docs or ask in Discord.
