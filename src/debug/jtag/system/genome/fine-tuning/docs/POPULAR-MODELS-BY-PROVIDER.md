# Popular Models for LoRA Fine-Tuning (From VRAM Calculator Data)

**Source**: apxml.com VRAM calculator - models people actually use for fine-tuning

This list represents real-world demand. These are the models users search for when planning fine-tuning jobs.

---

## Model Families & Provider Mapping

### **Meta Llama** (12 models) 🔥 HIGHEST PRIORITY
**Provider Support**:
- ✅ **OpenAI**: No (proprietary)
- ✅ **Together AI**: Yes (Llama 3.1 70B/8B, Llama 3.3 70B)
- ✅ **Fireworks**: Yes (Llama 3.1 405B/70B/8B)
- ✅ **DeepSeek**: No
- ⚠️ **Replicate**: Yes (community models)
- ⚠️ **Hugging Face**: Yes (all versions)

**Models**:
- Llama 3: 70B, 8B
- Llama 3.1: 405B, 70B, 8B
- Llama 3.2: 1B, 3B
- Llama 3.3: 70B
- Llama 4: Behemoth, Maverick, Scout (upcoming)

**Why Critical**: Most popular open model family, enterprise adoption, Meta backing

---

### **Qwen (Alibaba)** (24 models) 🔥 HIGHEST PRIORITY
**Provider Support**:
- ✅ **Qwen Cloud** (Alibaba): Native support (all models)
- ✅ **Together AI**: Qwen 2.5 72B
- ✅ **Fireworks**: Qwen 2.5 72B
- ⚠️ **Hugging Face**: All versions
- ⚠️ **Replicate**: Community models

**Models**:
- Qwen2: 0.5B, 1.5B, 7B, 72B
- Qwen2.5: 0.5B, 1.5B, 3B, 7B, 14B, 32B, 72B
- Qwen3: 0.6B, 1.7B, 4B, 8B, 14B, 32B, 235B-A22B (MoE), 30B-A3B (MoE)
- Qwen3 Coder: 480B-A35B (MoE)
- Qwen3 Thinking: 235B-A22B

**Why Critical**: Chinese market dominance, cost-effective, strong code/reasoning models, MoE architecture

**ACTION REQUIRED**: Add Qwen/Alibaba Cloud adapter (HIGH PRIORITY)

---

### **DeepSeek** (11 models) ✅ ALREADY SUPPORTED
**Provider Support**:
- ✅ **DeepSeek**: Native support (all models)
- ✅ **Fireworks**: DeepSeek V3.1
- ⚠️ **Hugging Face**: Community versions

**Models**:
- DeepSeek-R1: 1.5B, 3B, 7B, 8B, 14B, 32B, 70B, 671B (MoE)
- DeepSeek-V3: 671B
- DeepSeek-V3.1

**Status**: ✅ Already integrated (inference + fine-tuning)

---

### **Mistral AI** (8 models) ⚠️ PARTIALLY SUPPORTED
**Provider Support**:
- ✅ **Mistral**: Native support (all models)
- ✅ **Together AI**: Mixtral 8x7B, 8x22B
- ✅ **Fireworks**: Mixtral 8x7B
- ⚠️ **Hugging Face**: All versions

**Models**:
- Mistral-7B: v0.1, v0.2, Instruct v0.1, Instruct v0.2
- Mistral-Large: 2407
- Mistral-Small: 2501
- Ministral: 8B-2410
- Mixtral: 8x7B-v0.1, 8x22B-v0.1

**Status**: ⚠️ Have fine-tuning adapter, need inference adapter

**ACTION REQUIRED**: Add MistralAdapter.ts + MistralBaseConfig.ts

---

### **Google Gemma** (14 models) 🎯 HIGH PRIORITY
**Provider Support**:
- ✅ **Google Vertex AI**: Native support
- ⚠️ **Hugging Face**: All versions
- ⚠️ **Replicate**: Community models
- ❌ **No major inference providers** (opportunity!)

**Models**:
- Gemma 1: 2B, 7B
- Gemma 2: 2B, 9B, 27B
- Gemma 3: 270M, 1B, 4B, 12B, 27B
- Gemma 3n: E2B IT

**Why Important**: Google backing, license-friendly, efficient small models

**ACTION REQUIRED**: Research Google Vertex AI integration

---

### **Microsoft Phi** (10 models) 🎯 MEDIUM PRIORITY
**Provider Support**:
- ✅ **Azure AI**: Native support
- ⚠️ **Hugging Face**: All versions
- ❌ **No major inference providers**

**Models**:
- Phi-1, Phi-1.5, Phi-2
- Phi-3: mini, small, medium
- Phi-4, Phi-4-Mini
- Phi-4 Reasoning Plus

**Why Important**: Microsoft backing, excellent small models (3.8B beats 7B+ models)

**ACTION REQUIRED**: Research Azure AI integration

---

### **ChatGLM / GLM (Zhipu AI)** (12 models) 🇨🇳 CHINA PRIORITY
**Provider Support**:
- ✅ **Zhipu AI**: Native support
- ⚠️ **Hugging Face**: Community versions
- ❌ **Limited international presence**

**Models**:
- ChatGLM: 6B, ChatGLM2-6B, ChatGLM3-6B, ChatGLM3-6B-32K
- GLM-130B
- GLM-4, GLM-4.5, GLM-4.5-Air
- GLM-4-9B, GLM-4-9B-Chat, GLM-4-9B-Chat-1M
- GLM-4V (vision)

**Why Important**: Chinese market, bilingual (CN/EN), popular in Asia

**ACTION REQUIRED**: Research Zhipu AI API

---

### **Falcon (TII UAE)** (10 models) 🌍 INTERNATIONAL
**Provider Support**:
- ⚠️ **Hugging Face**: All versions
- ❌ **No major commercial providers**

**Models**:
- Falcon: 1B, 3B, 7B, 40B, 180B
- Falcon2: 11B
- Falcon3: 1B, 3B, 7B, 10B

**Why Important**: Middle East presence, open license, competitive performance

---

### **Cohere Command** (3 models) 💼 ENTERPRISE
**Provider Support**:
- ✅ **Cohere**: Native support
- ❌ **No fine-tuning on other platforms**

**Models**:
- Command A
- Command R
- Command R Plus

**Why Important**: Enterprise RAG focus, strong retrieval capabilities

**ACTION REQUIRED**: Research Cohere fine-tuning API

---

### **Other Chinese Models** 🇨🇳

#### **Baidu ERNIE** (10 models)
- ERNIE-4.5: 0.3B, 0.3B-Base, 21B-A3B, 21B-A3B-Base, 300B-A47B, 300B-A47B-Base
- ERNIE-4.5-VL (vision): 28B-A3B, 28B-A3B-Base, 424B-A47B, 424B-A47B-Base

**Provider**: Baidu Cloud
**Why Important**: Baidu's AI platform, Chinese enterprise market

#### **Moonshot Kimi** (6 models)
- Kimi-Dev-72B
- Kimi K2: Base, Instruct, Thinking
- Kimi Linear 48B A3B Instruct
- Kimi-VL: A3B-Instruct, A3B-Thinking

**Provider**: Moonshot AI
**Why Important**: Popular Chinese conversational AI

#### **Tencent Hunyuan** (4 models)
- Hunyuan: Lite, Standard, Large
- Hunyuan A13B

**Provider**: Tencent Cloud
**Why Important**: Tencent ecosystem, WeChat integration potential

#### **MiniMax M2** (1 model)
**Provider**: MiniMax
**Why Important**: Chinese market player

---

### **Other Notable Models**

#### **GPT-OSS** (2 models)
- GPT-OSS 20B, GPT-OSS 120B
**Note**: Open-source GPT-like models

#### **SmolLM3 3B** (1 model)
**Provider**: Hugging Face
**Why Important**: Efficient small model specialist

---

## Provider Priority Matrix

### Tier 1: MUST HAVE (Immediate Action)
1. **Qwen/Alibaba Cloud** - 24 models, Chinese market dominance
2. **Mistral AI** - European, enterprise-friendly
3. **Google Vertex AI** (Gemma) - 14 models, Google backing
4. **Cohere** - Enterprise RAG specialist

### Tier 2: HIGH VALUE (Next Quarter)
5. **Azure AI** (Phi) - Microsoft backing, best small models
6. **Zhipu AI** (GLM) - Chinese market, bilingual
7. **Baidu Cloud** (ERNIE) - Chinese enterprises
8. **Moonshot AI** (Kimi) - Chinese conversational AI
9. **Tencent Cloud** (Hunyuan) - WeChat ecosystem

### Tier 3: COMPLETENESS (Long-term)
10. Falcon (TII UAE) - International diversity
11. MiniMax - Chinese market coverage
12. Community models via Replicate/HuggingFace

---

## Market Insights

### Geographic Distribution
- **US/International**: Llama (Meta), OpenAI, Anthropic, Mistral
- **China**: Qwen, GLM, ERNIE, Kimi, Hunyuan (5 major providers!)
- **Europe**: Mistral AI
- **Middle East**: Falcon (UAE)

### Size Preferences
- **Tiny** (< 2B): Gemma 270M, Qwen3 0.6B, Phi-2 (edge deployment)
- **Small** (2B-10B): Phi-4, Gemma-9B, Mistral-7B (cost-effective)
- **Medium** (10B-100B): Llama 70B, Qwen 72B, GLM-130B (balanced)
- **Large** (100B-200B): Falcon-180B, Mistral-Large (power users)
- **MoE** (200B+): DeepSeek-V3 671B, Qwen3 235B-A22B (efficiency)

### Model Capabilities
- **General Chat**: Llama, Qwen, GLM, Command
- **Code**: DeepSeek-Coder, Qwen3-Coder, Phi (code-focused)
- **Reasoning**: DeepSeek-R1, Qwen3-Thinking, Phi-4-Reasoning
- **Vision**: GLM-4V, ERNIE-VL, Kimi-VL, Qwen-VL
- **Multilingual**: Qwen, GLM, Kimi (CN/EN), Mistral (EU languages)

---

## Action Items

### IMMEDIATE (This Week)
1. ✅ Fix Fireworks baseUrl
2. ✅ Fix Anthropic model version
3. 🔥 **Add Qwen/Alibaba Cloud adapter** - 24 models waiting!
4. 🔥 **Add Mistral inference adapter** - Complete existing fine-tuning support

### HIGH PRIORITY (Next 2 Weeks)
5. Add Google Vertex AI (Gemma) - 14 models
6. Add Cohere - Enterprise RAG market
7. Add Azure AI (Phi) - Best small models
8. Research Chinese providers (GLM, ERNIE, Kimi, Hunyuan)

### MEDIUM PRIORITY (Next Month)
9. Replicate integration - Community model repository
10. Enhanced Hugging Face support - Direct fine-tuning
11. Falcon models via Hugging Face
12. MiniMax research

---

## Competitive Intelligence

**What users want**:
1. **Llama** - Everyone wants this (Meta blessing)
2. **Qwen** - Chinese users + code specialists
3. **Small models** - Phi, Gemma (cost-conscious, edge deployment)
4. **Reasoning** - DeepSeek-R1, Qwen3-Thinking (latest trend)
5. **Vision** - GLM-4V, ERNIE-VL, Kimi-VL (multimodal future)

**Market gaps** (opportunities):
- No good Gemma fine-tuning platform (Google Vertex only)
- Phi models underserved (Azure only)
- Chinese model access for international users (opportunity!)
- Unified interface across all providers (OUR DIFFERENTIATOR)

**The winning strategy**: Support EVERYTHING, let users choose based on:
- Geography (US/China/EU regulations)
- Cost (Phi-2 vs GPT-4)
- Capability (reasoning, vision, code)
- Deployment (cloud, edge, on-premise)
