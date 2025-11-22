# Fine-Tuning Provider Research & Unified Adapter Architecture

**Research Date:** November 2, 2025
**Purpose:** Design a unified TypeScript adapter architecture supporting both local training (Apple Silicon MPS, CUDA, CPU) and remote APIs (OpenAI, Anthropic, Grok, Together.ai, etc.)

---

## Executive Summary

This document provides comprehensive research on available fine-tuning providers and APIs as of 2025, with detailed analysis of their capabilities, pricing, dataset formats, and API structures. The goal is to design a **unified fine-tuning adapter** that abstracts away provider-specific details while maintaining flexibility and type safety.

### Key Findings

1. **Standard Dataset Format**: JSONL with conversational messages format (`{"messages": [...]}`) is universally supported
2. **Common Job Lifecycle**: All providers use submit → poll → complete pattern
3. **LoRA Dominance**: LoRA (Low-Rank Adaptation) is the standard for parameter-efficient fine-tuning
4. **Adapter Format**: Safetensors is the emerging standard for LoRA weights (more secure than PyTorch .bin)
5. **Hybrid Approach**: Most teams will want local training for development + remote APIs for production

---

## Provider Comparison Table

| Provider | Models Available | Training Cost | Inference Cost | Adapter Download | Local Inference | API Maturity |
|----------|-----------------|---------------|----------------|------------------|-----------------|--------------|
| **OpenAI** | GPT-4o, GPT-4o-mini, GPT-3.5 | $3-25/M tokens | $0.30-15/M tokens | ❌ No (API-hosted only) | ❌ No | ⭐⭐⭐⭐⭐ |
| **Anthropic (Bedrock)** | Claude 3 Haiku | Not specified | Standard Bedrock rates | ❌ No (Bedrock-hosted) | ❌ No | ⭐⭐⭐⭐ |
| **Grok/X.AI** | Grok 4, Grok 3 | Not specified | Pay-per-request | ❌ No (API-hosted) | ❌ No | ⭐⭐⭐ |
| **Together.ai** | 50+ open models | Token-based | Token-based | ✅ Yes (optional) | ✅ Yes | ⭐⭐⭐⭐⭐ |
| **Replicate** | FLUX, Llama, Video | $0.001528/sec GPU | $0.36-43.92/hr | ❌ No (API-hosted) | ❌ No | ⭐⭐⭐⭐ |
| **Hugging Face AutoTrain** | Any HF model | Infrastructure cost | Self-hosted | ✅ Yes | ✅ Yes | ⭐⭐⭐⭐ |
| **Google Vertex AI** | Gemini 2.5, Gemini 2.0 | Token-based | Token-based | ❌ No (Vertex-hosted) | ❌ No | ⭐⭐⭐⭐ |
| **Cohere** | Command R | $3-8/M tokens | $2-4/M tokens | ❌ No (API-hosted) | ❌ No | ⭐⭐⭐⭐ |
| **Local PyTorch/PEFT** | Any supported | GPU/CPU time | Free | ✅ Yes | ✅ Yes | ⭐⭐⭐⭐⭐ |
| **Local MLX (Apple)** | Llama, Mistral, Phi, etc. | Mac GPU time | Free | ✅ Yes | ✅ Yes | ⭐⭐⭐⭐ |
| **Ollama** | N/A (uses pre-trained) | N/A | Free | ✅ Yes (GGUF) | ✅ Yes | ⭐⭐⭐⭐⭐ |

**Legend:**
- ⭐⭐⭐⭐⭐ = Production-ready, widely adopted
- ⭐⭐⭐⭐ = Stable, growing adoption
- ⭐⭐⭐ = Emerging, functional

---

## Detailed Provider Analysis

### 1. OpenAI Fine-Tuning API

**Status:** Production-ready, widely adopted
**Models:** GPT-4o, GPT-4o-mini, GPT-3.5 Turbo
**Fine-Tuning Access:** General availability (GPT-4 requires experimental access)

#### API Structure

**Authentication:**
```typescript
// Environment variable
process.env.OPENAI_API_KEY = "sk-...";

// Header format
headers: {
  "Authorization": `Bearer ${process.env.OPENAI_API_KEY}`,
  "Content-Type": "application/json"
}
```

**Endpoints:**
```typescript
// 1. Upload training file
POST https://api.openai.com/v1/files
Content-Type: multipart/form-data
Body: {
  file: <training.jsonl>,
  purpose: "fine-tune"
}
Response: { id: "file-abc123", ... }

// 2. Create fine-tuning job
POST https://api.openai.com/v1/fine_tuning/jobs
Body: {
  training_file: "file-abc123",
  model: "gpt-4o-mini-2024-07-18",
  hyperparameters: {
    n_epochs: 3,
    batch_size: 1,
    learning_rate_multiplier: 0.1
  }
}
Response: { id: "ftjob-xyz789", status: "pending", ... }

// 3. Check job status
GET https://api.openai.com/v1/fine_tuning/jobs/{job_id}
Response: {
  id: "ftjob-xyz789",
  status: "running" | "succeeded" | "failed",
  fine_tuned_model: "ft:gpt-4o-mini:org:model:abc123" // when completed
}

// 4. Use fine-tuned model
POST https://api.openai.com/v1/chat/completions
Body: {
  model: "ft:gpt-4o-mini:org:model:abc123",
  messages: [...]
}
```

#### Dataset Format

```jsonl
{"messages": [{"role": "system", "content": "You are an expert assistant."}, {"role": "user", "content": "What is LoRA?"}, {"role": "assistant", "content": "LoRA is Low-Rank Adaptation..."}]}
{"messages": [{"role": "system", "content": "You are an expert assistant."}, {"role": "user", "content": "Explain fine-tuning."}, {"role": "assistant", "content": "Fine-tuning is the process..."}]}
```

**Requirements:**
- Minimum 10 examples recommended
- JSONL format (JSON Lines)
- Conversational structure with `messages` array
- Roles: `system`, `user`, `assistant`
- Double quotes (not single quotes) for JSON validity

#### Pricing (2025)

**Training Costs:**
- GPT-4o-mini: $3.00 per million tokens
- GPT-4o: Not yet available for fine-tuning
- GPT-3.5 Turbo: $8.00 per million tokens

**Inference Costs (Fine-Tuned Models):**
- GPT-4o-mini: $0.30/M input, $1.20/M output
- GPT-3.5 Turbo: $3.00/M input, $6.00/M output

**Cost Calculation:**
```
Training cost = (dataset_tokens × epochs) × price_per_million
```

#### Adapter Output

**Format:** API-hosted only (no weight download)
**Inference:** Via OpenAI API with fine-tuned model ID
**Ownership:** You own the model, but it's hosted on OpenAI infrastructure

#### Job Lifecycle

```
pending → validating → running → succeeded/failed
```

**Status Polling:**
```typescript
const job = await openai.fineTuning.jobs.retrieve("ftjob-xyz789");
// Poll every 10-30 seconds until status === "succeeded"
```

---

### 2. Anthropic Claude (Amazon Bedrock)

**Status:** Generally available
**Models:** Claude 3 Haiku
**Fine-Tuning Access:** Via Amazon Bedrock only (US West Oregon region)

#### API Structure

**Platform:** Amazon Bedrock
**Authentication:** AWS IAM credentials
**Access:** Bedrock console or AWS SDK

```typescript
// AWS SDK example
import { BedrockClient, CreateModelCustomizationJobCommand } from "@aws-sdk/client-bedrock";

const client = new BedrockClient({ region: "us-west-2" });
const command = new CreateModelCustomizationJobCommand({
  jobName: "claude-haiku-custom",
  customModelName: "my-haiku-model",
  roleArn: "arn:aws:iam::...",
  baseModelIdentifier: "anthropic.claude-3-haiku",
  trainingDataConfig: {
    s3Uri: "s3://bucket/training-data.jsonl"
  },
  hyperParameters: {
    epochCount: "3",
    batchSize: "4",
    learningRate: "0.00001"
  }
});
```

#### Dataset Format

**Same as OpenAI:** JSONL with conversational messages
**Requirements:**
- Must follow Converse API message format
- System, user, and assistant messages
- Prompt-completion pairs representing ideal outputs

#### Pricing

**Training:** Not publicly disclosed (contact AWS)
**Inference:** Standard Bedrock pricing for Claude 3 Haiku
**Storage:** Monthly fee for customized model

**Note:** Fine-tuning pricing varies by use case; check AWS Bedrock pricing calculator.

#### Adapter Output

**Format:** Bedrock-hosted model
**Inference:** Via Bedrock API with custom model ARN
**Download:** Not available

#### Benefits

- Reduced cost for production (Haiku cheaper than Sonnet/Opus)
- Faster response times
- Improved accuracy on specialized tasks
- Enterprise security and compliance

---

### 3. Grok/X.AI Fine-Tuning API

**Status:** Emerging (API opened April 2025)
**Models:** Grok 4 Fast, Grok 3, Grok 3 Mini
**Fine-Tuning Access:** Custom fine-tunes supported via public API

#### API Structure

**Authentication:**
```typescript
headers: {
  "Authorization": `Bearer ${process.env.XAI_API_KEY}`,
  "Content-Type": "application/json"
}
```

**Endpoints:**
- Base URL: `https://api.x.ai/v1/`
- Fine-tuning endpoints not fully documented yet

**Capabilities:**
- Custom fine-tunes supported
- Retrieval-augmented workflows
- Parameter tuning for speed vs. depth

#### Dataset Format

**Likely:** JSONL conversational format (OpenAI-compatible)
**Confirmation needed:** Official documentation still limited

#### Pricing

**Not publicly disclosed**
**Expected:** Pay-per-request model similar to inference pricing

#### Adapter Output

**Format:** API-hosted (assumed)
**Inference:** Via X.AI API with fine-tuned model ID
**Download:** Unknown

#### Current Limitations

- Limited public documentation (as of Nov 2025)
- API still maturing
- Pricing not transparent

---

### 4. Together.ai Fine-Tuning API

**Status:** Production-ready, highly recommended
**Models:** 50+ open models (Llama, Mistral, DeepSeek, etc.)
**Fine-Tuning Methods:** LoRA, Full Fine-Tuning, DPO

#### API Structure

**Authentication:**
```bash
export TOGETHER_API_KEY="your-key"
```

**CLI Example:**
```bash
together fine-tuning create \
  --training-file "file-629e58b4-ff73-438c-b2cc-f69542b27980" \
  --model "meta-llama/Meta-Llama-3.1-8B-Instruct-Reference" \
  --lora
```

**Python Example:**
```python
import together

# Create fine-tuning job
job = together.fine_tuning.create(
    model="deepseek-ai/DeepSeek-R1-Distill-Qwen-14B",
    training_file="training-file.jsonl",
    lora=True  # Use LoRA for efficiency
)

# Check status
status = together.fine_tuning.retrieve(job.id)

# Use fine-tuned model
import requests

headers = {
    'Authorization': f'Bearer {os.environ.get("TOGETHER_API_KEY")}',
    'Content-Type': 'application/json'
}

payload = {
    "model": job.fine_tuned_model,
    "messages": [{"role": "user", "content": "Hello!"}],
    "max_tokens": 128
}

response = requests.post(
    "https://api.together.xyz/v1/chat/completions",
    headers=headers,
    json=payload
)
```

#### Dataset Format

**Formats Supported:**
- JSONL conversational (OpenAI-compatible)
- JSONL instruction-based
- CSV (converted internally)

**Example:**
```jsonl
{"messages": [{"role": "user", "content": "Question?"}, {"role": "assistant", "content": "Answer."}]}
```

**Requirements:**
- Must be consistent format (no mixing)
- Supports up to 32K context length (Llama 3.1 8B/70B)
- Validation errors shown before upload

#### Pricing (2025)

**Training Cost:**
- Calculated as: `(training_tokens × epochs) + (validation_tokens × evaluations)`
- Price varies by model size and method (LoRA vs. Full FT)
- **No minimum charge** - pay only for tokens processed

**Inference Cost:**
- Serverless: Pay per token
- Dedicated endpoints: Hourly rate
- Multi-LoRA: Deploy hundreds of adapters with single base model

**Recent Updates (April 2025):**
- Lower training costs
- Browser-based fine-tuning (no code required)
- Pay-as-you-go with no minimums

#### Adapter Output

**Format:** LoRA adapters in safetensors/Hugging Face format
**Download:** ✅ **Yes** - Adapter weights can be downloaded
**Inference Options:**
1. Together.ai API (hosted)
2. Local inference (download adapters)
3. Hugging Face Hub deployment

**Multi-LoRA:**
- Load/unload adapters dynamically
- Serve hundreds of custom models efficiently

#### Key Advantages

✅ Adapter download supported
✅ 50+ base models to choose from
✅ LoRA and full fine-tuning
✅ DPO for preference alignment
✅ Transparent pricing
✅ No minimums
✅ Local + cloud flexibility

**Recommended for:** Teams wanting control over adapters with cloud convenience

---

### 5. Replicate Fine-Tuning API

**Status:** Production-ready (focused on image/video models)
**Models:** FLUX.1, Llama, HunyuanVideo, custom models
**Fine-Tuning Focus:** Image generation, video generation, LLMs

#### API Structure

**Authentication:**
```bash
export REPLICATE_API_TOKEN="r8_..."
```

**Creating a Model:**
```bash
curl -X POST https://api.replicate.com/v1/models \
  -H "Authorization: Bearer $REPLICATE_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "owner": "your-username",
    "name": "my-fine-tuned-model",
    "visibility": "private",
    "hardware": "gpu-a100"
  }'
```

**Training via API:**
```python
import replicate

training = replicate.trainings.create(
    version="owner/model:version",
    input={
        "input_images": "https://example.com/images.zip",
        "steps": 1000
    },
    destination="your-username/my-model"
)

# Poll for completion
while training.status != "succeeded":
    training.reload()
    time.sleep(10)
```

**Running Fine-Tuned Model:**
```python
output = replicate.run(
    "your-username/my-model:version",
    input={"prompt": "A photo of a person"}
)
```

#### Dataset Format

**Varies by model type:**
- **Images:** ZIP file of training images
- **LLMs:** JSONL conversational format
- **Video:** Video files + captions

#### Pricing (2025)

**Training:**
- **FLUX.1 example:** ~20 min training = $1.85 USD
- **GPU rate:** $0.001528/second on H100
- **Varies by hardware:** CPU ($0.36/hr) to 8x H100 ($43.92/hr)

**Inference:**
- Billed per prediction (time-based)
- Fast-booting fine-tunes: Only pay when active
- No idle charges

**Free Tier:**
- New users get free compute credits

#### Adapter Output

**Format:** Model hosted on Replicate
**Download:** ❌ No (weights not accessible)
**Inference:** Via Replicate API only

#### Key Advantages

✅ Excellent for image/video models
✅ Fast training times
✅ Simple API
✅ Cost-effective for experimentation
❌ No adapter download
❌ Locked into Replicate platform

**Recommended for:** Image generation, video models, quick prototyping

---

### 6. Hugging Face AutoTrain

**Status:** Mature, open-source
**Models:** Any Hugging Face Hub model
**Fine-Tuning Methods:** SFT, DPO, ORPO, LoRA

#### API Structure

**Installation:**
```bash
pip install autotrain-advanced
```

**CLI Usage:**
```bash
autotrain llm \
  --train \
  --model meta-llama/Llama-2-7b-hf \
  --project-name my-finetuned-model \
  --data-path ./data \
  --text-column text \
  --lr 2e-4 \
  --batch-size 4 \
  --epochs 3 \
  --trainer sft \
  --peft \
  --quantization int4
```

**Python API:**
```python
from autotrain import AutoTrain

trainer = AutoTrain(
    model="mistralai/Mistral-7B-v0.1",
    task="llm:sft",
    data_path="./data.csv",
    project_name="my-model",
    config={
        "learning_rate": 2e-4,
        "num_epochs": 3,
        "batch_size": 4,
        "peft": True,
        "quantization": "int4"
    }
)

trainer.train()
```

#### Dataset Format

**Supports:**
- CSV
- JSONL

**Example CSV:**
```csv
text
"<s>[INST] Question? [/INST] Answer.</s>"
"<s>[INST] Another question? [/INST] Another answer.</s>"
```

#### Pricing

**AutoTrain is FREE** (open-source)

**Infrastructure Costs:**
- **Local:** Your own GPU/CPU
- **Hugging Face Spaces:** Pay for compute time
- **Cloud:** Your cloud provider rates

#### Adapter Output

**Format:** Hugging Face PEFT format (safetensors)
**Download:** ✅ **Yes** - Full control
**Inference Options:**
1. Local (transformers + PEFT)
2. Hugging Face Inference API
3. Self-hosted

**Files Created:**
```
output/
├── adapter_config.json
├── adapter_model.safetensors  # LoRA weights (~6MB)
├── tokenizer.json
└── training_args.json
```

#### Key Advantages

✅ **FREE and open-source**
✅ Complete control over training
✅ Supports all HF models
✅ Multiple training methods (SFT, DPO, ORPO)
✅ Quantization support (int4, int8)
✅ Local or cloud
❌ Requires GPU setup
❌ More technical than hosted solutions

**Recommended for:** Developers who want full control, researchers, cost-sensitive projects

---

### 7. Google Vertex AI (Gemini Fine-Tuning)

**Status:** Production-ready
**Models:** Gemini 2.5 Pro/Flash/Lite, Gemini 2.0 Flash
**Fine-Tuning Method:** Supervised fine-tuning

#### API Structure

**Authentication:** Google Cloud credentials

**Python SDK:**
```python
from vertexai.preview.tuning import sft

# Create tuning job
job = sft.train(
    source_model="gemini-2.0-flash-001",
    train_dataset="gs://bucket/train.jsonl",
    validation_dataset="gs://bucket/val.jsonl",
    epochs=3,
    adapter_size=4,
    learning_rate=0.001
)

# Check status
job.refresh()
print(job.state)  # PENDING, RUNNING, SUCCEEDED, FAILED

# Use tuned model
from vertexai.generative_models import GenerativeModel

model = GenerativeModel(job.tuned_model_name)
response = model.generate_content("Hello!")
```

#### Dataset Format

**JSONL conversational format:**
```jsonl
{"messages": [{"role": "user", "content": "Question"}, {"role": "model", "content": "Answer"}]}
```

**Requirements:**
- Minimum 100 examples recommended
- Quality > quantity
- System, user, and model messages

#### Pricing

**Training:**
- Token-based: `tokens_in_dataset × epochs × price_per_token`
- Price per token not publicly disclosed (check Vertex AI pricing)

**Inference:**
- Standard Vertex AI rates for tuned models
- Typically higher than base model rates

#### Adapter Output

**Format:** Vertex AI-hosted model
**Download:** ❌ No
**Inference:** Via Vertex AI API only

#### Key Features

✅ Latest Gemini models
✅ Automatic metrics (loss, token accuracy)
✅ Flexible dataset sizes (100-1000s)
✅ Google Cloud integration
❌ No adapter download
❌ Locked into GCP

**Recommended for:** Teams already on Google Cloud, Gemini users

---

### 8. Cohere Fine-Tuning API

**Status:** Production-ready
**Models:** Command R
**Fine-Tuning Method:** LoRA

#### API Structure

**Authentication:**
```python
import cohere

co = cohere.Client(api_key="your-api-key")
```

**Create Fine-Tune:**
```python
finetune = co.finetuning.create_finetuned_model(
    request={
        "name": "my-model",
        "settings": {
            "base_model": "command-r",
            "train_file": {"id": "file-123"},
            "epochs": 3,
            "learning_rate": 0.0001
        }
    }
)

# Check status
status = co.finetuning.get_finetuned_model(finetune.id)

# Use fine-tuned model
response = co.chat(
    model=finetune.id,
    message="Hello!"
)
```

#### Dataset Format

**JSONL conversational:**
```jsonl
{"messages": [{"role": "User", "content": "Question?"}, {"role": "Chatbot", "content": "Answer."}]}
```

#### Pricing (2025)

**Training:**
- $3.00-8.00 per million tokens (sources vary)

**Inference (Command R Fine-Tuned):**
- Input: $2.00 per million tokens
- Output: $4.00 per million tokens

#### Adapter Output

**Format:** API-hosted
**Download:** ❌ No
**Inference:** Via Cohere API

#### Key Advantages

✅ Transparent pricing
✅ Simple API
✅ Good documentation
❌ Limited to Command R
❌ No adapter download

**Recommended for:** Teams using Cohere's ecosystem

---

### 9. Local Fine-Tuning: PyTorch + PEFT (CUDA)

**Status:** Gold standard for research/development
**Models:** Any PyTorch-compatible model
**Fine-Tuning Methods:** LoRA, QLoRA, Full fine-tuning, DPO, PPO

#### Requirements

**Hardware:**
- **Minimum:** 8GB VRAM (7B model with QLoRA + 4-bit quantization)
- **Recommended:** 16-24GB VRAM (7B model with LoRA)
- **Optimal:** 24GB+ VRAM (larger models, faster training)

**Software:**
```bash
pip install torch==2.1.2 transformers==4.36.2 datasets==2.16.1 \
  bitsandbytes==0.42.0 peft==0.7.1 accelerate trl
```

**CUDA:** Version 12.3 recommended

#### Code Example

```python
from transformers import AutoModelForCausalLM, AutoTokenizer
from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
from datasets import load_dataset
from trl import SFTTrainer

# Load model with quantization
model = AutoModelForCausalLM.from_pretrained(
    "meta-llama/Llama-2-7b-hf",
    load_in_4bit=True,  # QLoRA
    device_map="auto"
)

# Configure LoRA
lora_config = LoraConfig(
    r=16,  # Rank
    lora_alpha=32,
    target_modules=["q_proj", "v_proj"],
    lora_dropout=0.05,
    bias="none",
    task_type="CAUSAL_LM"
)

model = prepare_model_for_kbit_training(model)
model = get_peft_model(model, lora_config)

# Load dataset
dataset = load_dataset("json", data_files="train.jsonl")

# Train
trainer = SFTTrainer(
    model=model,
    train_dataset=dataset["train"],
    max_seq_length=2048,
    args={
        "per_device_train_batch_size": 4,
        "num_train_epochs": 3,
        "learning_rate": 2e-4,
        "output_dir": "./output"
    }
)

trainer.train()

# Save adapter
model.save_pretrained("./lora-adapter")

# Load and merge later
from peft import PeftModel

base_model = AutoModelForCausalLM.from_pretrained("meta-llama/Llama-2-7b-hf")
model = PeftModel.from_pretrained(base_model, "./lora-adapter")
merged_model = model.merge_and_unload()  # Optional: merge to base
```

#### Dataset Format

**JSONL with text column:**
```jsonl
{"text": "<s>[INST] Question? [/INST] Answer.</s>"}
```

**Or conversational:**
```jsonl
{"messages": [{"role": "user", "content": "Q?"}, {"role": "assistant", "content": "A."}]}
```

#### Pricing

**FREE** (uses your own hardware)

**Time Investment:**
- 7B model on 24GB GPU: ~1-2 hours for 1000 samples
- Depends on model size, dataset size, epochs

#### Adapter Output

**Format:** Safetensors (PEFT format)
**Files Created:**
```
output/
├── adapter_config.json
├── adapter_model.safetensors  # ~6-50MB
└── training_args.json
```

**Inference:**
1. Local with transformers + PEFT
2. Convert to GGUF for Ollama/llama.cpp
3. Deploy to Hugging Face Hub
4. Use with vLLM for production serving

#### Key Advantages

✅ **Full control**
✅ **No API costs**
✅ **Privacy (data never leaves your machine)**
✅ **Experiment freely**
✅ **Supports all techniques (LoRA, QLoRA, DPO, PPO)**
✅ **Adapter weights fully portable**
❌ Requires GPU setup
❌ Slower than cloud (for large datasets)
❌ Electricity costs

**Recommended for:** Researchers, privacy-sensitive projects, heavy experimentation

---

### 10. Local Fine-Tuning: MLX (Apple Silicon)

**Status:** Rapidly maturing (2025)
**Models:** Llama, Mistral, Phi, Qwen, Gemma, many others
**Fine-Tuning Method:** LoRA, QLoRA

#### Requirements

**Hardware:**
- **Minimum:** M1/M2 with 16GB RAM
- **Recommended:** M2 Pro/Max or M3 with 16GB+ RAM
- **Optimal:** M3 Max/Ultra with 32GB+ unified memory

**Software:**
```bash
pip install mlx-lm
```

#### Code Example

```bash
# Fine-tune using MLX
mlx_lm.lora \
  --model meta-llama/Llama-2-7b-hf \
  --train \
  --data ./data \
  --iters 1000 \
  --batch-size 4 \
  --lora-layers 16 \
  --learning-rate 1e-5 \
  --save-every 100 \
  --output ./adapters

# Fuse adapter into base model (optional)
mlx_lm.fuse \
  --model meta-llama/Llama-2-7b-hf \
  --adapter ./adapters \
  --output ./fused-model

# Inference
mlx_lm.generate \
  --model ./fused-model \
  --prompt "Hello, world!" \
  --max-tokens 100
```

**Python API:**
```python
from mlx_lm import load, generate
from mlx_lm.tuner import train

# Train
train(
    model="mistralai/Mistral-7B-v0.1",
    data="./data",
    train=True,
    iters=1000,
    batch_size=4,
    lora_layers=16,
    learning_rate=1e-5,
    adapter_path="./adapters"
)

# Load and use
model, tokenizer = load("mistralai/Mistral-7B-v0.1", adapter_path="./adapters")
response = generate(model, tokenizer, prompt="Hello!", max_tokens=100)
```

#### Dataset Format

**JSONL:**
```jsonl
{"text": "<s>[INST] Question [/INST] Answer</s>"}
```

#### Pricing

**FREE** (uses your Mac)

**Performance:**
- **7B model on M3 Pro:** ~10 minutes for small dataset
- **Unified memory:** Very efficient (no VRAM bottleneck)

#### Adapter Output

**Format:** MLX adapters.safetensors
**Files Created:**
```
adapters/
├── adapters.safetensors  # LoRA weights
├── adapter_config.json
└── ...
```

**Conversion:**
- Can convert to Hugging Face PEFT format
- Can fuse into base model weights

**Inference:**
1. MLX framework (native)
2. Convert to Hugging Face format
3. Convert to GGUF for Ollama

#### Key Advantages

✅ **Optimized for Apple Silicon**
✅ **Fast training (10-30 min for 7B)**
✅ **Unified memory = efficient**
✅ **No cloud costs**
✅ **Privacy**
✅ **Supports quantized training (QLoRA)**
❌ Mac-only
❌ Adapter format conversion needed for cross-platform

**Recommended for:** Mac users, rapid iteration, local development

---

### 11. Ollama (Inference with Pre-Trained LoRA)

**Status:** Mature, widely adopted
**Focus:** Local inference (not training)
**Adapter Support:** ✅ Yes (GGUF format)

#### Overview

Ollama **does not train models**, but it **can load and serve LoRA adapters** that were trained elsewhere.

#### Workflow

```bash
# 1. Train adapter (using MLX, PEFT, etc.)
# 2. Convert to GGUF format
python llama.cpp/convert-lora-to-ggml.py \
  --base-model ./base-model \
  --lora-adapter ./adapters \
  --output ggml-adapter-model.bin

# 3. Create Modelfile
cat > Modelfile <<EOF
FROM tinyllama:latest
ADAPTER ./ggml-adapter-model.bin
PARAMETER temperature 0.7
EOF

# 4. Create Ollama model
ollama create my-custom-model -f Modelfile

# 5. Run locally
ollama run my-custom-model "Your prompt here"
```

#### Adapter Format

**Required:** GGUF format (from llama.cpp)
**Source:** Can come from:
- Unsloth (train → export to GGUF)
- MLX (train → convert to GGUF)
- PEFT (train → convert via llama.cpp)

#### Pricing

**FREE** (local inference)

#### Key Advantages

✅ **Fast local inference**
✅ **Supports LoRA adapters**
✅ **Easy to use**
✅ **No cloud costs**
✅ **Privacy**
❌ **Does not train** (must train elsewhere)
❌ **GGUF conversion required**

**Recommended for:** Local serving of custom models, production inference on-prem

---

## Common Patterns Across Providers

### 1. Standard Dataset Format

**Universal Format:** JSONL with conversational messages

```jsonl
{"messages": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "Hello!"}, {"role": "assistant", "content": "Hi there!"}]}
```

**Key Takeaway:** Design your unified adapter to generate this format, and you'll be compatible with 90% of providers.

### 2. Training Job Lifecycle

**Universal Pattern:**

```
1. Upload Dataset → file_id
2. Create Job → job_id
3. Poll Status → pending | validating | running | succeeded | failed
4. Use Model → model_id or adapter
```

**Polling Implementation:**
```typescript
async function waitForJobCompletion(jobId: string): Promise<string> {
  let status = 'pending';
  while (status !== 'succeeded' && status !== 'failed') {
    await sleep(10000); // 10 seconds
    const job = await checkJobStatus(jobId);
    status = job.status;
    console.log(`Job ${jobId}: ${status}`);
  }
  if (status === 'failed') {
    throw new Error(`Job failed: ${job.error}`);
  }
  return job.fine_tuned_model;
}
```

### 3. LoRA Dominance

**Why LoRA is Standard:**
- **Efficient:** Only trains small adapter (~6-50MB) instead of full model (7-70GB)
- **Fast:** 10x-100x faster training
- **Portable:** Adapter can be swapped between base models
- **Cost-effective:** Requires less compute

**Variants:**
- **LoRA:** Standard (rank 8-64)
- **QLoRA:** Quantized (4-bit/8-bit) for lower memory
- **Multi-LoRA:** Load multiple adapters dynamically

### 4. Safetensors Format

**Why Safetensors:**
- **Secure:** No pickle (no arbitrary code execution)
- **Fast:** Lazy loading, memory-mapped
- **Standard:** Adopted by Hugging Face, transformers, PEFT

**Files in LoRA Adapter:**
```
adapter_model.safetensors  # Weights (A and B matrices)
adapter_config.json        # Config (rank, alpha, target modules)
```

**Size:** Typically 6-50MB (vs. 7-70GB full model)

### 5. Error Handling Patterns

**Common Errors:**
- **Validation errors:** Dataset format incorrect
- **Quota errors:** Rate limits, token limits
- **Timeout errors:** Job took too long
- **OOM errors:** Model too large for available memory

**Unified Error Handling:**
```typescript
class FineTuneError extends Error {
  constructor(
    message: string,
    public code: string,
    public provider: string,
    public retriable: boolean
  ) {
    super(message);
  }
}

// Usage
if (error.code === 'quota_exceeded') {
  throw new FineTuneError(
    'API quota exceeded',
    'quota_exceeded',
    'openai',
    true  // Can retry later
  );
}
```

### 6. Cost Estimation Pattern

**Formula (Token-Based Providers):**
```
Training Cost = (tokens_in_dataset × epochs × price_per_million) / 1,000,000
```

**Example:**
- Dataset: 100K tokens
- Epochs: 3
- Price: $3.00/M tokens (GPT-4o-mini)
- **Cost:** `(100,000 × 3 × 3.00) / 1,000,000 = $0.90`

**Unified Cost Estimator:**
```typescript
interface CostEstimate {
  trainingCost: number;
  inferenceCostPer1M: { input: number; output: number };
  currency: 'USD';
  provider: string;
}

function estimateCost(
  provider: string,
  tokensInDataset: number,
  epochs: number
): CostEstimate {
  const pricing = PRICING_TABLE[provider];
  return {
    trainingCost: (tokensInDataset * epochs * pricing.train) / 1_000_000,
    inferenceCostPer1M: pricing.inference,
    currency: 'USD',
    provider
  };
}
```

---

## Recommended Unified Interface Design

### Core Abstractions

```typescript
// 1. Provider-agnostic fine-tuning config
interface FineTuneConfig {
  provider: 'openai' | 'anthropic' | 'together' | 'local-peft' | 'local-mlx';
  baseModel: string;
  dataset: Dataset;
  hyperparameters: {
    epochs: number;
    batchSize?: number;
    learningRate?: number;
    loraRank?: number;
    loraAlpha?: number;
  };
  outputPath?: string;  // For local training
  validation?: {
    enabled: boolean;
    splitRatio?: number;
  };
}

// 2. Universal dataset format
interface Dataset {
  format: 'jsonl' | 'csv' | 'parquet';
  path: string;  // File path or URL
  conversations: Conversation[];
}

interface Conversation {
  messages: Message[];
}

interface Message {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

// 3. Job status tracking
interface FineTuneJob {
  id: string;
  provider: string;
  status: 'pending' | 'validating' | 'running' | 'succeeded' | 'failed';
  progress?: number;  // 0-100
  fineTunedModel?: string;  // Model ID or path to adapter
  error?: string;
  createdAt: Date;
  updatedAt: Date;
  estimatedCost?: number;
  actualCost?: number;
}

// 4. Adapter metadata
interface LoRAAdapter {
  id: string;
  name: string;
  baseModel: string;
  provider: string;
  format: 'safetensors' | 'gguf' | 'api-hosted';
  path?: string;  // For local adapters
  apiModelId?: string;  // For API-hosted adapters
  size: number;  // Bytes
  rank: number;
  alpha: number;
  targetModules: string[];
  createdAt: Date;
}
```

### Unified Adapter Interface

```typescript
interface IFineTuneProvider {
  // 1. Initialize provider
  initialize(config: ProviderConfig): Promise<void>;

  // 2. Upload dataset
  uploadDataset(dataset: Dataset): Promise<string>;  // Returns dataset ID

  // 3. Create fine-tuning job
  createJob(config: FineTuneConfig): Promise<FineTuneJob>;

  // 4. Poll job status
  getJobStatus(jobId: string): Promise<FineTuneJob>;

  // 5. Cancel job (if supported)
  cancelJob(jobId: string): Promise<void>;

  // 6. Download adapter (if supported)
  downloadAdapter(jobId: string, outputPath: string): Promise<LoRAAdapter>;

  // 7. Inference with fine-tuned model
  generateText(
    modelId: string,
    prompt: string,
    options?: InferenceOptions
  ): Promise<string>;

  // 8. Cost estimation
  estimateCost(
    config: FineTuneConfig
  ): Promise<CostEstimate>;
}
```

### Implementation Example

```typescript
class UnifiedFineTuneAdapter {
  private providers: Map<string, IFineTuneProvider> = new Map();

  constructor() {
    // Register providers
    this.providers.set('openai', new OpenAIProvider());
    this.providers.set('together', new TogetherAIProvider());
    this.providers.set('local-peft', new LocalPEFTProvider());
    this.providers.set('local-mlx', new LocalMLXProvider());
  }

  // Unified fine-tuning method
  async fineTune(config: FineTuneConfig): Promise<LoRAAdapter> {
    const provider = this.providers.get(config.provider);
    if (!provider) {
      throw new Error(`Provider ${config.provider} not supported`);
    }

    // 1. Validate and convert dataset to universal format
    const dataset = await this.prepareDataset(config.dataset);

    // 2. Estimate cost
    const costEstimate = await provider.estimateCost(config);
    console.log(`Estimated cost: $${costEstimate.trainingCost.toFixed(2)}`);

    // 3. Upload dataset
    const datasetId = await provider.uploadDataset(dataset);

    // 4. Create job
    const job = await provider.createJob({
      ...config,
      dataset: { ...dataset, path: datasetId }
    });

    // 5. Poll for completion
    let currentJob = job;
    while (currentJob.status === 'pending' || currentJob.status === 'running') {
      await sleep(10000);
      currentJob = await provider.getJobStatus(job.id);
      console.log(`Job ${job.id}: ${currentJob.status} (${currentJob.progress}%)`);
    }

    if (currentJob.status === 'failed') {
      throw new Error(`Fine-tuning failed: ${currentJob.error}`);
    }

    // 6. Download adapter (if supported)
    if (config.outputPath && provider.downloadAdapter) {
      return await provider.downloadAdapter(job.id, config.outputPath);
    }

    // 7. Return adapter metadata
    return {
      id: currentJob.id,
      name: config.baseModel + '-finetuned',
      baseModel: config.baseModel,
      provider: config.provider,
      format: config.provider.startsWith('local') ? 'safetensors' : 'api-hosted',
      apiModelId: currentJob.fineTunedModel,
      size: 0,  // Unknown for API-hosted
      rank: config.hyperparameters.loraRank || 16,
      alpha: config.hyperparameters.loraAlpha || 32,
      targetModules: ['q_proj', 'v_proj'],
      createdAt: new Date()
    };
  }

  // Convert any dataset format to universal JSONL
  private async prepareDataset(dataset: Dataset): Promise<Dataset> {
    // Normalize to JSONL conversational format
    const normalized: Conversation[] = dataset.conversations.map(conv => ({
      messages: conv.messages.map(msg => ({
        role: this.normalizeRole(msg.role),
        content: msg.content
      }))
    }));

    return {
      format: 'jsonl',
      path: dataset.path,
      conversations: normalized
    };
  }

  private normalizeRole(role: string): 'system' | 'user' | 'assistant' {
    const lower = role.toLowerCase();
    if (lower === 'model' || lower === 'assistant' || lower === 'chatbot') {
      return 'assistant';
    }
    if (lower === 'user' || lower === 'human') {
      return 'user';
    }
    return 'system';
  }

  // Inference abstraction
  async generate(
    adapter: LoRAAdapter,
    prompt: string,
    options?: InferenceOptions
  ): Promise<string> {
    const provider = this.providers.get(adapter.provider);
    if (!provider) {
      throw new Error(`Provider ${adapter.provider} not supported`);
    }

    if (adapter.format === 'api-hosted' && adapter.apiModelId) {
      return await provider.generateText(adapter.apiModelId, prompt, options);
    }

    if (adapter.format === 'safetensors' && adapter.path) {
      // Load locally with PEFT or MLX
      return await this.generateLocally(adapter, prompt, options);
    }

    throw new Error(`Cannot generate: adapter format ${adapter.format} not supported`);
  }

  private async generateLocally(
    adapter: LoRAAdapter,
    prompt: string,
    options?: InferenceOptions
  ): Promise<string> {
    if (adapter.provider === 'local-mlx') {
      return await this.generateWithMLX(adapter, prompt, options);
    }
    return await this.generateWithPEFT(adapter, prompt, options);
  }

  private async generateWithMLX(
    adapter: LoRAAdapter,
    prompt: string,
    options?: InferenceOptions
  ): Promise<string> {
    // Use mlx_lm.generate
    const { exec } = require('child_process');
    return new Promise((resolve, reject) => {
      exec(
        `mlx_lm.generate --model ${adapter.baseModel} --adapter ${adapter.path} --prompt "${prompt}" --max-tokens ${options?.maxTokens || 100}`,
        (error: any, stdout: string) => {
          if (error) reject(error);
          else resolve(stdout.trim());
        }
      );
    });
  }

  private async generateWithPEFT(
    adapter: LoRAAdapter,
    prompt: string,
    options?: InferenceOptions
  ): Promise<string> {
    // Python subprocess or native Node.js binding
    // For now, shell out to Python script
    const { exec } = require('child_process');
    return new Promise((resolve, reject) => {
      exec(
        `python3 scripts/generate_peft.py --base-model ${adapter.baseModel} --adapter ${adapter.path} --prompt "${prompt}"`,
        (error: any, stdout: string) => {
          if (error) reject(error);
          else resolve(stdout.trim());
        }
      );
    });
  }
}
```

### Usage Example

```typescript
// Initialize adapter
const adapter = new UnifiedFineTuneAdapter();

// Fine-tune with Together.ai
const togetherAdapter = await adapter.fineTune({
  provider: 'together',
  baseModel: 'meta-llama/Llama-3.1-8B-Instruct',
  dataset: {
    format: 'jsonl',
    path: './training-data.jsonl',
    conversations: [
      {
        messages: [
          { role: 'user', content: 'What is LoRA?' },
          { role: 'assistant', content: 'LoRA is Low-Rank Adaptation...' }
        ]
      }
    ]
  },
  hyperparameters: {
    epochs: 3,
    batchSize: 4,
    learningRate: 2e-4,
    loraRank: 16,
    loraAlpha: 32
  },
  outputPath: './adapters/together-lora'
});

// Fine-tune locally with MLX
const mlxAdapter = await adapter.fineTune({
  provider: 'local-mlx',
  baseModel: 'mistralai/Mistral-7B-v0.1',
  dataset: {
    format: 'jsonl',
    path: './training-data.jsonl',
    conversations: [/* ... */]
  },
  hyperparameters: {
    epochs: 3,
    loraRank: 16
  },
  outputPath: './adapters/mlx-lora'
});

// Inference
const response = await adapter.generate(
  togetherAdapter,
  'Explain fine-tuning.',
  { maxTokens: 200 }
);

console.log(response);
```

---

## Implementation Priority

### Phase 1: Core Infrastructure (Week 1)
1. ✅ **Unified dataset format** (JSONL conversational)
2. ✅ **Job status tracking** (database schema)
3. ✅ **Cost estimation** (pricing table + calculator)
4. ✅ **Base provider interface** (`IFineTuneProvider`)

### Phase 2: High-Value Providers (Week 2-3)
1. **Together.ai** (best balance: cloud + adapter download)
2. **Local MLX** (Apple Silicon, for Mac users)
3. **Local PEFT** (CUDA, for researchers)

**Why this order:**
- Together.ai gives cloud convenience + adapter control
- Local MLX/PEFT give privacy + cost savings
- Covers 80% of use cases

### Phase 3: Enterprise Providers (Week 4)
1. **OpenAI** (most requested, despite no adapter download)
2. **Anthropic Bedrock** (enterprise users)
3. **Google Vertex AI** (Gemini users)

### Phase 4: Specialized Providers (Week 5+)
1. **Replicate** (image/video models)
2. **Cohere** (specific use cases)
3. **Grok/X.AI** (when docs improve)

### Phase 5: Advanced Features (Week 6+)
1. **Multi-LoRA paging** (load/unload adapters dynamically)
2. **Adapter format conversion** (safetensors ↔ GGUF ↔ PEFT)
3. **Continuous learning** (incremental fine-tuning)
4. **DPO/PPO training** (preference alignment)
5. **Distributed training** (multi-GPU, multi-node)

---

## Key Insights for Architecture

### 1. Hybrid Approach is Optimal

**Strategy:**
- **Development:** Local training (MLX/PEFT) for fast iteration
- **Production:** Cloud APIs (Together.ai/OpenAI) for scale
- **Privacy:** Local training + local inference (Ollama)

**Why:**
- Local training is cheap and private
- Cloud APIs scale better for production
- Together.ai bridges both worlds (train in cloud, download adapter)

### 2. LoRA is the Universal Adapter Format

**Architecture Decision:**
- All adapters stored as LoRA (safetensors format)
- Paging system loads/unloads adapters from base model
- Conversion tools for GGUF (Ollama) and API-hosted (OpenAI)

**Benefits:**
- Small size (~6-50MB)
- Fast swapping (<1 second)
- Compatible with all major frameworks

### 3. Dataset Normalization is Critical

**Problem:** Every provider has slightly different format requirements

**Solution:** Unified preprocessing pipeline
```
User Data → Normalize → Validate → Convert to JSONL → Upload
```

**Normalization:**
- Convert all role names to standard (`user`, `assistant`, `system`)
- Ensure double quotes (JSON validity)
- Add system prompt if missing
- Split long conversations if needed

### 4. Cost Transparency Matters

**User Need:** "How much will this fine-tuning job cost?"

**Solution:** Pre-flight cost estimation
```typescript
const estimate = await adapter.estimateCost(config);
console.log(`Estimated cost: $${estimate.trainingCost.toFixed(2)}`);

// User confirmation
if (estimate.trainingCost > 10.00) {
  const confirmed = await askUser('Proceed with fine-tuning?');
  if (!confirmed) return;
}
```

### 5. Job Status Polling Needs Backoff

**Problem:** Polling every second wastes API calls

**Solution:** Exponential backoff
```typescript
async function pollWithBackoff(jobId: string): Promise<FineTuneJob> {
  let delay = 5000;  // Start at 5 seconds
  const maxDelay = 60000;  // Cap at 60 seconds

  while (true) {
    const job = await getJobStatus(jobId);
    if (job.status === 'succeeded' || job.status === 'failed') {
      return job;
    }

    await sleep(delay);
    delay = Math.min(delay * 1.5, maxDelay);  // Increase by 50% each time
  }
}
```

### 6. Adapter Registry for Multi-Backend Support

**Architecture:**
```typescript
class AdapterRegistry {
  private adapters: Map<string, LoRAAdapter> = new Map();

  register(adapter: LoRAAdapter): void {
    this.adapters.set(adapter.id, adapter);
  }

  get(adapterId: string): LoRAAdapter | undefined {
    return this.adapters.get(adapterId);
  }

  listByDomain(domain: string): LoRAAdapter[] {
    return Array.from(this.adapters.values())
      .filter(a => a.name.includes(domain));
  }

  // LRU eviction when memory pressure
  evictLRU(): void {
    const sorted = Array.from(this.adapters.values())
      .sort((a, b) => a.lastUsedAt - b.lastUsedAt);
    const toEvict = sorted[0];
    this.adapters.delete(toEvict.id);
  }
}
```

### 7. Error Recovery Patterns

**Common Failures:**
1. **Validation errors** → Fix dataset format
2. **Quota errors** → Wait and retry
3. **Timeout errors** → Use smaller model or dataset
4. **OOM errors** → Use quantization (QLoRA)

**Implementation:**
```typescript
async function fineTuneWithRetry(config: FineTuneConfig): Promise<LoRAAdapter> {
  const maxRetries = 3;
  let lastError: Error;

  for (let i = 0; i < maxRetries; i++) {
    try {
      return await adapter.fineTune(config);
    } catch (error) {
      lastError = error;

      if (error.retriable) {
        const delay = 2 ** i * 10000;  // Exponential backoff
        console.log(`Retry ${i + 1}/${maxRetries} after ${delay}ms`);
        await sleep(delay);
      } else {
        throw error;  // Non-retriable error
      }
    }
  }

  throw lastError;
}
```

---

## Conclusion

This research provides a comprehensive foundation for building a unified fine-tuning adapter architecture. The key takeaways:

1. **Start with Together.ai + Local MLX/PEFT** - covers 80% of use cases
2. **Use JSONL conversational format** - universally compatible
3. **Design for LoRA adapters** - efficient and portable
4. **Build cost estimation upfront** - users need transparency
5. **Plan for hybrid local/cloud** - different needs at different stages

The proposed unified interface abstracts provider-specific details while maintaining type safety and flexibility. This architecture supports the PersonaUser genome paging vision: load skill-specific adapters on-demand, evict when memory pressure, and continuously fine-tune as just another task type.

**Next Steps:**
1. Implement core interfaces (Phase 1)
2. Build Together.ai provider (Phase 2)
3. Build Local MLX provider (Phase 2)
4. Test with PersonaUser integration (Phase 2)
5. Add remaining providers iteratively (Phase 3+)

---

**Document Version:** 1.0
**Last Updated:** November 2, 2025
**Maintainer:** Claude Code
