#!/usr/bin/env bash
#
# RunPod Gate Gradient Capture Pipeline
#
# Spins up an A100 80GB pod, runs LoRA training with GateGradientCallback
# on a target model using coding data, then downloads gate_gradients.json
# for local plasticity compaction.
#
# RESUMABLE: If interrupted, re-run with --resume=<OUTPUT_DIR> to pick up
# where it left off (reattaches to existing pod, skips completed phases).
#
# Usage:
#   ./runpod-gate-capture.sh [model] [dataset]
#   ./runpod-gate-capture.sh --resume=~/.continuum/gate-captures/Qwen-Qwen2.5-Coder-32B-Instruct-1742123456
#
# Examples:
#   ./runpod-gate-capture.sh Qwen/Qwen2.5-Coder-32B-Instruct
#   ./runpod-gate-capture.sh Qwen/Qwen2.5-Coder-14B-Instruct /path/to/coding.jsonl
#
# Requirements:
#   - RUNPOD_API_KEY in ~/.continuum/config.env or environment
#   - HF_TOKEN in ~/.continuum/config.env or environment (for gated models)
#   - jq installed (brew install jq)
#   - ssh-keygen (for SSH key if not exists)

set -euo pipefail

# ── Parse --resume flag ──────────────────────────────────────────────
RESUME_DIR=""
for arg in "$@"; do
    case "$arg" in
        --resume=*) RESUME_DIR="${arg#--resume=}"; shift ;;
    esac
done

# ── Config ─────────────────────────────────────────────────────────────
MODEL="${1:-Qwen/Qwen2.5-Coder-32B-Instruct}"
DATASET_PATH="${2:-}"  # Empty = generate coding dataset on-pod
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [[ -n "$RESUME_DIR" ]]; then
    OUTPUT_DIR="$RESUME_DIR"
    echo "🔄 Resuming from: $OUTPUT_DIR"
    # Reload model from checkpoint
    if [[ -f "$OUTPUT_DIR/checkpoint.json" ]]; then
        MODEL=$(jq -r '.model' "$OUTPUT_DIR/checkpoint.json")
    fi
else
    OUTPUT_DIR="$HOME/.continuum/gate-captures/$(echo "$MODEL" | tr '/' '-')-$(date +%s)"
fi

# Training hyperparams — optimized for gate gradient capture, not model quality.
# We want ENOUGH steps to get stable per-head utilization scores, not a good adapter.
RANK=16          # Lower rank = faster training, still captures gradients
ALPHA=32
EPOCHS=2         # 2 epochs is enough for gradient signal
BATCH_SIZE=2     # Small batch on 80GB = fine
LEARNING_RATE="2e-4"
QUANTIZE_BITS=4  # QLoRA: fit 32B in 80GB VRAM

# RunPod config — GPU auto-selected based on model size
# 32B+ needs 80GB VRAM (A100), smaller models use cheapest available (A6000 $0.33/hr)
if echo "$MODEL" | grep -qE '32B|34B|35B|70B|72B'; then
    GPU_TYPE_ID="${GPU_TYPE_ID:-NVIDIA A100 80GB PCIe}"  # $1.19/hr
    VOLUME_DISK=100  # GB — 32B models are ~60GB
else
    GPU_TYPE_ID="${GPU_TYPE_ID:-NVIDIA RTX A6000}"  # $0.33/hr, 48GB
    VOLUME_DISK=50   # GB
fi
CONTAINER_DISK=20   # GB
# PyTorch 2.4 devel image — pre-cached on most RunPod machines (instant start)
# Has PyTorch+CUDA pre-installed, just need pip install for HF libs
DOCKER_IMAGE="runpod/pytorch:2.4.0-py3.11-cuda12.4.1-devel-ubuntu22.04"

# ── Load secrets ───────────────────────────────────────────────────────
CONFIG_ENV="$HOME/.continuum/config.env"
if [[ -f "$CONFIG_ENV" ]]; then
    RUNPOD_API_KEY="${RUNPOD_API_KEY:-$(grep '^RUNPOD_API_KEY=' "$CONFIG_ENV" | cut -d= -f2-)}"
    HF_TOKEN="${HF_TOKEN:-$(grep '^HF_TOKEN=' "$CONFIG_ENV" | cut -d= -f2-)}"
fi

if [[ -z "${RUNPOD_API_KEY:-}" ]]; then
    echo "RUNPOD_API_KEY not found in environment or $CONFIG_ENV"
    exit 1
fi

# ── Ensure SSH key exists ──────────────────────────────────────────────
SSH_KEY="$HOME/.ssh/id_ed25519"
if [[ ! -f "$SSH_KEY" ]]; then
    echo "Generating SSH key..."
    ssh-keygen -t ed25519 -f "$SSH_KEY" -N "" -q
fi
SSH_PUB_KEY="$(cat "${SSH_KEY}.pub")"

# ── Checkpoint system ────────────────────────────────────────────────
# Phases: dataset_ready, pod_created, pod_running, files_uploaded,
#         deps_installed, training_complete, results_downloaded
save_checkpoint() {
    local phase="$1"
    shift
    local extra_json="${1:-}"
    mkdir -p "$OUTPUT_DIR"
    # Build checkpoint JSON
    local checkpoint="{\"phase\":\"$phase\",\"model\":\"$MODEL\",\"timestamp\":$(date +%s)"
    if [[ -n "${POD_ID:-}" ]]; then
        checkpoint="$checkpoint,\"podId\":\"$POD_ID\""
    fi
    if [[ -n "${SSH_HOST:-}" ]]; then
        checkpoint="$checkpoint,\"sshHost\":\"$SSH_HOST\",\"sshPort\":$SSH_PORT"
    fi
    if [[ -n "$extra_json" ]]; then
        checkpoint="$checkpoint,$extra_json"
    fi
    checkpoint="$checkpoint}"
    echo "$checkpoint" > "$OUTPUT_DIR/checkpoint.json"
    echo "  [checkpoint: $phase]"
}

load_checkpoint() {
    if [[ -f "$OUTPUT_DIR/checkpoint.json" ]]; then
        jq -r '.phase' "$OUTPUT_DIR/checkpoint.json"
    else
        echo "none"
    fi
}

phase_complete() {
    local target="$1"
    local current
    current=$(load_checkpoint)
    # Phase ordering
    local -a phases=(none dataset_ready pod_created pod_running files_uploaded deps_installed training_complete results_downloaded)
    local current_idx=0 target_idx=0
    for i in "${!phases[@]}"; do
        if [[ "${phases[$i]}" == "$current" ]]; then current_idx=$i; fi
        if [[ "${phases[$i]}" == "$target" ]]; then target_idx=$i; fi
    done
    [[ $current_idx -ge $target_idx ]]
}

# ── GraphQL API ──────────────────────────────────────────────────────
RUNPOD_GQL="https://api.runpod.io/graphql"

runpod_gql() {
    local query="$1"
    curl -s -X POST "$RUNPOD_GQL" \
        -H "Authorization: Bearer ${RUNPOD_API_KEY}" \
        -H "Content-Type: application/json" \
        -d "$query"
}

create_pod() {
    local escaped_hf_token
    escaped_hf_token=$(echo -n "${HF_TOKEN:-}" | sed 's/"/\\"/g')

    # SSH key is registered at account level (updateUserSettings) — no need for PUBLIC_KEY env var
    local query
    query=$(cat <<GRAPHQL
{
  "query": "mutation { podFindAndDeployOnDemand(input: { name: \"gate-capture-$(date +%s)\", imageName: \"$DOCKER_IMAGE\", gpuTypeId: \"$GPU_TYPE_ID\", cloudType: COMMUNITY, supportPublicIp: true, startSsh: true, gpuCount: 1, containerDiskInGb: $CONTAINER_DISK, volumeInGb: $VOLUME_DISK, volumeMountPath: \"/workspace\", minVcpuCount: 4, minMemoryInGb: 32, ports: \"22/tcp\", env: [{ key: \"HF_TOKEN\", value: \"$escaped_hf_token\" }, { key: \"TRANSFORMERS_CACHE\", value: \"/workspace/.cache\" }, { key: \"HF_HOME\", value: \"/workspace/.huggingface\" }] }) { id imageName machineId machine { podHostId } } }"
}
GRAPHQL
)
    runpod_gql "$query"
}

query_pod() {
    local pod_id="$1"
    local query
    query=$(cat <<GRAPHQL
{
  "query": "query { pod(input: { podId: \"$pod_id\" }) { id name desiredStatus costPerHr runtime { ports { ip isIpPublic privatePort publicPort type } } machine { gpuDisplayName } } }"
}
GRAPHQL
)
    runpod_gql "$query"
}

terminate_pod() {
    local pod_id="$1"
    local query
    query=$(cat <<GRAPHQL
{
  "query": "mutation { podTerminate(input: { podId: \"$pod_id\" }) }"
}
GRAPHQL
)
    runpod_gql "$query"
}

wait_for_pod() {
    local pod_id="$1"
    echo -n "Waiting for pod to be ready"
    for i in $(seq 1 120); do
        local result
        result=$(query_pod "$pod_id")
        # Pod is ready when runtime.ports contains SSH (privatePort 22)
        local ssh_port
        ssh_port=$(echo "$result" | jq -r '.data.pod.runtime.ports[]? | select(.privatePort == 22) | .publicPort // empty' 2>/dev/null)

        if [[ -n "$ssh_port" ]]; then
            echo " READY (SSH available)"
            return 0
        fi
        echo -n "."
        sleep 5
    done
    echo " TIMED OUT after 10 minutes"
    return 1
}

get_ssh_connection() {
    local pod_id="$1"
    local result
    result=$(query_pod "$pod_id")

    # Extract SSH port info from runtime.ports
    local ssh_info
    ssh_info=$(echo "$result" | jq -r '
        .data.pod.runtime.ports[]
        | select(.privatePort == 22)
        | .ip + " " + (.publicPort | tostring)
    ' 2>/dev/null | head -1)

    echo "$ssh_info"
}

cleanup_pod() {
    if [[ -z "${POD_ID:-}" ]]; then return; fi
    # Only auto-terminate if we haven't downloaded results yet
    if phase_complete "results_downloaded"; then
        echo "Terminating pod ${POD_ID}..."
        terminate_pod "$POD_ID" > /dev/null 2>&1 || true
        echo "Pod terminated"
    else
        echo ""
        echo "WARNING: Pod $POD_ID is still running (training may still be in progress)"
        echo "  Resume:    $0 --resume=$OUTPUT_DIR"
        echo "  Terminate: curl -s -X POST $RUNPOD_GQL -H 'Authorization: Bearer \$RUNPOD_API_KEY' -H 'Content-Type: application/json' -d '{\"query\":\"mutation { podTerminate(input: { podId: \\\"$POD_ID\\\" }) }\"}'"
        echo ""
        echo "Pod left running to allow resume. You are being billed."
    fi
}

# ── Prepare coding dataset ─────────────────────────────────────────────
mkdir -p "$OUTPUT_DIR"

if ! phase_complete "dataset_ready"; then
    if [[ -z "$DATASET_PATH" ]]; then
        echo "Generating coding training dataset for gate gradient capture..."
        DATASET_PATH="$OUTPUT_DIR/coding-dataset.jsonl"

        python3 -c "
import json, sys

examples = [
    # Algorithm implementation
    {'messages': [
        {'role': 'user', 'content': 'Write a Python function to find the longest common subsequence of two strings.'},
        {'role': 'assistant', 'content': 'def lcs(s1, s2):\n    m, n = len(s1), len(s2)\n    dp = [[0] * (n + 1) for _ in range(m + 1)]\n    for i in range(1, m + 1):\n        for j in range(1, n + 1):\n            if s1[i-1] == s2[j-1]:\n                dp[i][j] = dp[i-1][j-1] + 1\n            else:\n                dp[i][j] = max(dp[i-1][j], dp[i][j-1])\n    result = []\n    i, j = m, n\n    while i > 0 and j > 0:\n        if s1[i-1] == s2[j-1]:\n            result.append(s1[i-1])\n            i -= 1\n            j -= 1\n        elif dp[i-1][j] > dp[i][j-1]:\n            i -= 1\n        else:\n            j -= 1\n    return \"\".join(reversed(result))'}
    ]},
    # Data structure
    {'messages': [
        {'role': 'user', 'content': 'Implement a min-heap in Python with insert and extract_min operations.'},
        {'role': 'assistant', 'content': 'class MinHeap:\n    def __init__(self):\n        self.heap = []\n\n    def parent(self, i):\n        return (i - 1) // 2\n\n    def left(self, i):\n        return 2 * i + 1\n\n    def right(self, i):\n        return 2 * i + 2\n\n    def insert(self, key):\n        self.heap.append(key)\n        i = len(self.heap) - 1\n        while i > 0 and self.heap[self.parent(i)] > self.heap[i]:\n            self.heap[i], self.heap[self.parent(i)] = self.heap[self.parent(i)], self.heap[i]\n            i = self.parent(i)\n\n    def extract_min(self):\n        if not self.heap:\n            raise IndexError(\"empty heap\")\n        if len(self.heap) == 1:\n            return self.heap.pop()\n        root = self.heap[0]\n        self.heap[0] = self.heap.pop()\n        self._heapify(0)\n        return root\n\n    def _heapify(self, i):\n        smallest = i\n        l, r = self.left(i), self.right(i)\n        if l < len(self.heap) and self.heap[l] < self.heap[smallest]:\n            smallest = l\n        if r < len(self.heap) and self.heap[r] < self.heap[smallest]:\n            smallest = r\n        if smallest != i:\n            self.heap[i], self.heap[smallest] = self.heap[smallest], self.heap[i]\n            self._heapify(smallest)'}
    ]},
    # Graph algorithm
    {'messages': [
        {'role': 'user', 'content': 'Write Dijkstra\\'s shortest path algorithm in Python.'},
        {'role': 'assistant', 'content': 'import heapq\nfrom collections import defaultdict\n\ndef dijkstra(graph, start):\n    dist = {start: 0}\n    pq = [(0, start)]\n    visited = set()\n    prev = {}\n\n    while pq:\n        d, u = heapq.heappop(pq)\n        if u in visited:\n            continue\n        visited.add(u)\n        for v, w in graph[u]:\n            if v not in visited:\n                new_dist = d + w\n                if new_dist < dist.get(v, float(\"inf\")):\n                    dist[v] = new_dist\n                    prev[v] = u\n                    heapq.heappush(pq, (new_dist, v))\n    return dist, prev'}
    ]},
    # String manipulation
    {'messages': [
        {'role': 'user', 'content': 'Write a function to check if a string is a valid palindrome considering only alphanumeric characters.'},
        {'role': 'assistant', 'content': 'def is_palindrome(s: str) -> bool:\n    cleaned = \"\".join(c.lower() for c in s if c.isalnum())\n    return cleaned == cleaned[::-1]'}
    ]},
    # Tree traversal
    {'messages': [
        {'role': 'user', 'content': 'Implement iterative inorder traversal of a binary tree.'},
        {'role': 'assistant', 'content': 'def inorder_iterative(root):\n    result = []\n    stack = []\n    current = root\n    while current or stack:\n        while current:\n            stack.append(current)\n            current = current.left\n        current = stack.pop()\n        result.append(current.val)\n        current = current.right\n    return result'}
    ]},
    # Sorting
    {'messages': [
        {'role': 'user', 'content': 'Implement merge sort in Python.'},
        {'role': 'assistant', 'content': 'def merge_sort(arr):\n    if len(arr) <= 1:\n        return arr\n    mid = len(arr) // 2\n    left = merge_sort(arr[:mid])\n    right = merge_sort(arr[mid:])\n    return merge(left, right)\n\ndef merge(left, right):\n    result = []\n    i = j = 0\n    while i < len(left) and j < len(right):\n        if left[i] <= right[j]:\n            result.append(left[i])\n            i += 1\n        else:\n            result.append(right[j])\n            j += 1\n    result.extend(left[i:])\n    result.extend(right[j:])\n    return result'}
    ]},
    # Dynamic programming
    {'messages': [
        {'role': 'user', 'content': 'Solve the 0/1 knapsack problem using dynamic programming.'},
        {'role': 'assistant', 'content': 'def knapsack(weights, values, capacity):\n    n = len(weights)\n    dp = [[0] * (capacity + 1) for _ in range(n + 1)]\n    for i in range(1, n + 1):\n        for w in range(capacity + 1):\n            dp[i][w] = dp[i-1][w]\n            if weights[i-1] <= w:\n                dp[i][w] = max(dp[i][w], dp[i-1][w - weights[i-1]] + values[i-1])\n    # Backtrack to find items\n    items = []\n    w = capacity\n    for i in range(n, 0, -1):\n        if dp[i][w] != dp[i-1][w]:\n            items.append(i-1)\n            w -= weights[i-1]\n    return dp[n][capacity], items'}
    ]},
    # Concurrency
    {'messages': [
        {'role': 'user', 'content': 'Write a thread-safe bounded queue in Python using threading primitives.'},
        {'role': 'assistant', 'content': 'import threading\nfrom collections import deque\n\nclass BoundedQueue:\n    def __init__(self, maxsize):\n        self.queue = deque()\n        self.maxsize = maxsize\n        self.mutex = threading.Lock()\n        self.not_full = threading.Condition(self.mutex)\n        self.not_empty = threading.Condition(self.mutex)\n\n    def put(self, item):\n        with self.not_full:\n            while len(self.queue) >= self.maxsize:\n                self.not_full.wait()\n            self.queue.append(item)\n            self.not_empty.notify()\n\n    def get(self):\n        with self.not_empty:\n            while not self.queue:\n                self.not_empty.wait()\n            item = self.queue.popleft()\n            self.not_full.notify()\n            return item\n\n    def __len__(self):\n        with self.mutex:\n            return len(self.queue)'}
    ]},
    # Error handling patterns
    {'messages': [
        {'role': 'user', 'content': 'Implement a retry decorator with exponential backoff in Python.'},
        {'role': 'assistant', 'content': 'import time\nimport functools\n\ndef retry(max_retries=3, base_delay=1, backoff_factor=2, exceptions=(Exception,)):\n    def decorator(func):\n        @functools.wraps(func)\n        def wrapper(*args, **kwargs):\n            delay = base_delay\n            for attempt in range(max_retries + 1):\n                try:\n                    return func(*args, **kwargs)\n                except exceptions as e:\n                    if attempt == max_retries:\n                        raise\n                    time.sleep(delay)\n                    delay *= backoff_factor\n        return wrapper\n    return decorator'}
    ]},
    # Database/SQL
    {'messages': [
        {'role': 'user', 'content': 'Write a simple connection pool implementation in Python for SQLite.'},
        {'role': 'assistant', 'content': 'import sqlite3\nimport threading\nfrom queue import Queue\n\nclass ConnectionPool:\n    def __init__(self, database, pool_size=5):\n        self.database = database\n        self.pool = Queue(maxsize=pool_size)\n        for _ in range(pool_size):\n            conn = sqlite3.connect(database, check_same_thread=False)\n            self.pool.put(conn)\n\n    def get_connection(self):\n        return self.pool.get()\n\n    def return_connection(self, conn):\n        self.pool.put(conn)\n\n    def __enter__(self):\n        self.conn = self.get_connection()\n        return self.conn\n\n    def __exit__(self, exc_type, exc_val, exc_tb):\n        self.return_connection(self.conn)'}
    ]},
    # Parser
    {'messages': [
        {'role': 'user', 'content': 'Write a recursive descent parser for simple arithmetic expressions (+ - * / with parentheses).'},
        {'role': 'assistant', 'content': 'class Parser:\n    def __init__(self, text):\n        self.text = text\n        self.pos = 0\n\n    def parse(self):\n        result = self.expr()\n        if self.pos != len(self.text):\n            raise SyntaxError(f\"Unexpected char at {self.pos}\")\n        return result\n\n    def expr(self):\n        result = self.term()\n        while self.pos < len(self.text) and self.text[self.pos] in \"+-\":\n            op = self.text[self.pos]\n            self.pos += 1\n            right = self.term()\n            result = result + right if op == \"+\" else result - right\n        return result\n\n    def term(self):\n        result = self.factor()\n        while self.pos < len(self.text) and self.text[self.pos] in \"*/\":\n            op = self.text[self.pos]\n            self.pos += 1\n            right = self.factor()\n            result = result * right if op == \"*\" else result / right\n        return result\n\n    def factor(self):\n        if self.text[self.pos] == \"(\":\n            self.pos += 1\n            result = self.expr()\n            self.pos += 1  # skip )\n            return result\n        start = self.pos\n        while self.pos < len(self.text) and (self.text[self.pos].isdigit() or self.text[self.pos] == \".\"):\n            self.pos += 1\n        return float(self.text[start:self.pos])'}
    ]},
    # Trie
    {'messages': [
        {'role': 'user', 'content': 'Implement a Trie with insert, search, and startsWith methods.'},
        {'role': 'assistant', 'content': 'class TrieNode:\n    def __init__(self):\n        self.children = {}\n        self.is_end = False\n\nclass Trie:\n    def __init__(self):\n        self.root = TrieNode()\n\n    def insert(self, word: str) -> None:\n        node = self.root\n        for char in word:\n            if char not in node.children:\n                node.children[char] = TrieNode()\n            node = node.children[char]\n        node.is_end = True\n\n    def search(self, word: str) -> bool:\n        node = self._find(word)\n        return node is not None and node.is_end\n\n    def starts_with(self, prefix: str) -> bool:\n        return self._find(prefix) is not None\n\n    def _find(self, prefix: str):\n        node = self.root\n        for char in prefix:\n            if char not in node.children:\n                return None\n            node = node.children[char]\n        return node'}
    ]},
    # Rust code
    {'messages': [
        {'role': 'user', 'content': 'Write a Rust function that reads a file and counts word frequencies, returning a HashMap.'},
        {'role': 'assistant', 'content': 'use std::collections::HashMap;\nuse std::fs;\nuse std::io;\n\nfn count_words(path: &str) -> io::Result<HashMap<String, usize>> {\n    let content = fs::read_to_string(path)?;\n    let mut counts = HashMap::new();\n    for word in content.split_whitespace() {\n        let word = word.to_lowercase();\n        let word = word.trim_matches(|c: char| !c.is_alphanumeric());\n        if !word.is_empty() {\n            *counts.entry(word.to_string()).or_insert(0) += 1;\n        }\n    }\n    Ok(counts)\n}'}
    ]},
    # TypeScript
    {'messages': [
        {'role': 'user', 'content': 'Write a TypeScript generic LRU cache with O(1) get and put operations.'},
        {'role': 'assistant', 'content': 'class LRUCache<K, V> {\n  private capacity: number;\n  private cache: Map<K, V>;\n\n  constructor(capacity: number) {\n    this.capacity = capacity;\n    this.cache = new Map();\n  }\n\n  get(key: K): V | undefined {\n    if (!this.cache.has(key)) return undefined;\n    const value = this.cache.get(key)!;\n    // Move to end (most recently used)\n    this.cache.delete(key);\n    this.cache.set(key, value);\n    return value;\n  }\n\n  put(key: K, value: V): void {\n    if (this.cache.has(key)) {\n      this.cache.delete(key);\n    } else if (this.cache.size >= this.capacity) {\n      // Evict least recently used (first entry)\n      const firstKey = this.cache.keys().next().value;\n      if (firstKey !== undefined) this.cache.delete(firstKey);\n    }\n    this.cache.set(key, value);\n  }\n\n  get size(): number {\n    return this.cache.size;\n  }\n}'}
    ]},
    # Network programming
    {'messages': [
        {'role': 'user', 'content': 'Write an async HTTP client in Python that makes concurrent requests with rate limiting.'},
        {'role': 'assistant', 'content': 'import asyncio\nimport aiohttp\nfrom typing import List, Dict, Any\n\nclass RateLimitedClient:\n    def __init__(self, max_concurrent: int = 10, requests_per_second: float = 5.0):\n        self.semaphore = asyncio.Semaphore(max_concurrent)\n        self.delay = 1.0 / requests_per_second\n        self.last_request = 0.0\n        self.lock = asyncio.Lock()\n\n    async def _wait_for_rate_limit(self):\n        async with self.lock:\n            now = asyncio.get_event_loop().time()\n            elapsed = now - self.last_request\n            if elapsed < self.delay:\n                await asyncio.sleep(self.delay - elapsed)\n            self.last_request = asyncio.get_event_loop().time()\n\n    async def fetch(self, session: aiohttp.ClientSession, url: str) -> Dict[str, Any]:\n        async with self.semaphore:\n            await self._wait_for_rate_limit()\n            async with session.get(url) as response:\n                return {\"url\": url, \"status\": response.status, \"body\": await response.text()}\n\n    async def fetch_all(self, urls: List[str]) -> List[Dict[str, Any]]:\n        async with aiohttp.ClientSession() as session:\n            tasks = [self.fetch(session, url) for url in urls]\n            return await asyncio.gather(*tasks, return_exceptions=True)'}
    ]},
    # Testing
    {'messages': [
        {'role': 'user', 'content': 'Write a property-based testing framework in Python (like a mini Hypothesis).'},
        {'role': 'assistant', 'content': 'import random\nimport functools\n\nclass PropertyTestFailure(Exception):\n    def __init__(self, args, exception):\n        self.failing_args = args\n        self.original_exception = exception\n        super().__init__(f\"Property failed for args={args}: {exception}\")\n\ndef given(**strategies):\n    def decorator(test_func):\n        @functools.wraps(test_func)\n        def wrapper(num_examples=100):\n            for i in range(num_examples):\n                kwargs = {name: strategy() for name, strategy in strategies.items()}\n                try:\n                    test_func(**kwargs)\n                except Exception as e:\n                    # Attempt shrinking\n                    shrunk = _shrink(test_func, kwargs, strategies)\n                    raise PropertyTestFailure(shrunk or kwargs, e)\n        return wrapper\n    return decorator\n\ndef _shrink(test_func, failing_kwargs, strategies, attempts=50):\n    smallest = failing_kwargs.copy()\n    for _ in range(attempts):\n        candidate = {}\n        for name, val in smallest.items():\n            if isinstance(val, int):\n                candidate[name] = val // 2 if val != 0 else 0\n            elif isinstance(val, str):\n                candidate[name] = val[:len(val)//2]\n            elif isinstance(val, list):\n                candidate[name] = val[:len(val)//2]\n            else:\n                candidate[name] = val\n        try:\n            test_func(**candidate)\n        except Exception:\n            smallest = candidate\n    return smallest\n\n# Strategies\ndef integers(min_val=-1000, max_val=1000):\n    return lambda: random.randint(min_val, max_val)\n\ndef text(min_size=0, max_size=100):\n    return lambda: \"\".join(random.choices(\"abcdefghijklmnopqrstuvwxyz \", k=random.randint(min_size, max_size)))'}
    ]},
    # Systems programming
    {'messages': [
        {'role': 'user', 'content': 'Implement a simple memory allocator (first-fit) in C.'},
        {'role': 'assistant', 'content': '#include <stddef.h>\n#include <stdint.h>\n\ntypedef struct block_header {\n    size_t size;\n    int free;\n    struct block_header *next;\n} block_header;\n\nstatic char heap[65536];\nstatic block_header *free_list = NULL;\nstatic int initialized = 0;\n\nvoid init_allocator(void) {\n    free_list = (block_header *)heap;\n    free_list->size = sizeof(heap) - sizeof(block_header);\n    free_list->free = 1;\n    free_list->next = NULL;\n    initialized = 1;\n}\n\nvoid *my_malloc(size_t size) {\n    if (!initialized) init_allocator();\n    size = (size + 7) & ~7;  // Align to 8 bytes\n    block_header *curr = free_list;\n    while (curr) {\n        if (curr->free && curr->size >= size) {\n            if (curr->size >= size + sizeof(block_header) + 8) {\n                block_header *new_block = (block_header *)((char *)curr + sizeof(block_header) + size);\n                new_block->size = curr->size - size - sizeof(block_header);\n                new_block->free = 1;\n                new_block->next = curr->next;\n                curr->size = size;\n                curr->next = new_block;\n            }\n            curr->free = 0;\n            return (void *)((char *)curr + sizeof(block_header));\n        }\n        curr = curr->next;\n    }\n    return NULL;\n}\n\nvoid my_free(void *ptr) {\n    if (!ptr) return;\n    block_header *header = (block_header *)((char *)ptr - sizeof(block_header));\n    header->free = 1;\n    // Coalesce adjacent free blocks\n    block_header *curr = free_list;\n    while (curr && curr->next) {\n        if (curr->free && curr->next->free) {\n            curr->size += sizeof(block_header) + curr->next->size;\n            curr->next = curr->next->next;\n        } else {\n            curr = curr->next;\n        }\n    }\n}'}
    ]},
    # ML/data processing
    {'messages': [
        {'role': 'user', 'content': 'Implement k-means clustering from scratch in Python using only numpy.'},
        {'role': 'assistant', 'content': 'import numpy as np\n\ndef kmeans(X, k, max_iters=100, tol=1e-4):\n    n, d = X.shape\n    # Initialize centroids using k-means++\n    centroids = np.empty((k, d))\n    centroids[0] = X[np.random.randint(n)]\n    for i in range(1, k):\n        dists = np.min([np.sum((X - c) ** 2, axis=1) for c in centroids[:i]], axis=0)\n        probs = dists / dists.sum()\n        centroids[i] = X[np.random.choice(n, p=probs)]\n\n    for iteration in range(max_iters):\n        # Assign points to nearest centroid\n        dists = np.sqrt(((X[:, np.newaxis] - centroids[np.newaxis]) ** 2).sum(axis=2))\n        labels = np.argmin(dists, axis=1)\n\n        # Update centroids\n        new_centroids = np.array([X[labels == i].mean(axis=0) if np.any(labels == i) else centroids[i] for i in range(k)])\n\n        # Check convergence\n        shift = np.sqrt(((new_centroids - centroids) ** 2).sum())\n        centroids = new_centroids\n        if shift < tol:\n            break\n\n    inertia = sum(np.sum((X[labels == i] - centroids[i]) ** 2) for i in range(k))\n    return labels, centroids, inertia'}
    ]},
    # Bit manipulation
    {'messages': [
        {'role': 'user', 'content': 'Implement a bitmap (bitset) data structure in Python supporting set, clear, test, and count operations for up to 1M bits.'},
        {'role': 'assistant', 'content': 'class Bitmap:\n    def __init__(self, size):\n        self.size = size\n        self.words = bytearray((size + 7) // 8)\n\n    def set(self, pos):\n        if 0 <= pos < self.size:\n            self.words[pos >> 3] |= (1 << (pos & 7))\n\n    def clear(self, pos):\n        if 0 <= pos < self.size:\n            self.words[pos >> 3] &= ~(1 << (pos & 7))\n\n    def test(self, pos):\n        if 0 <= pos < self.size:\n            return bool(self.words[pos >> 3] & (1 << (pos & 7)))\n        return False\n\n    def count(self):\n        return sum(bin(byte).count(\"1\") for byte in self.words)\n\n    def __and__(self, other):\n        result = Bitmap(min(self.size, other.size))\n        for i in range(len(result.words)):\n            result.words[i] = self.words[i] & other.words[i]\n        return result\n\n    def __or__(self, other):\n        result = Bitmap(max(self.size, other.size))\n        for i in range(min(len(self.words), len(other.words))):\n            result.words[i] = self.words[i] | other.words[i]\n        return result'}
    ]},
]

with open('$DATASET_PATH', 'w') as f:
    for ex in examples:
        f.write(json.dumps(ex) + '\n')
print(f'Generated {len(examples)} coding examples')
" || { echo "Failed to generate dataset"; exit 1; }
    else
        # Copy user-provided dataset to output dir for checkpoint consistency
        cp "$DATASET_PATH" "$OUTPUT_DIR/coding-dataset.jsonl"
    fi
    save_checkpoint "dataset_ready"
else
    echo "Skipping dataset generation (already done)"
    DATASET_PATH="$OUTPUT_DIR/coding-dataset.jsonl"
fi

EXAMPLE_COUNT=$(wc -l < "$DATASET_PATH" | tr -d ' ')
echo "Dataset: $DATASET_PATH ($EXAMPLE_COUNT examples)"

# ── Create training config ─────────────────────────────────────────────
CONFIG_PATH="$OUTPUT_DIR/train-config.json"
if [[ ! -f "$CONFIG_PATH" ]]; then
    cat > "$CONFIG_PATH" << EOF
{
    "baseModel": "$MODEL",
    "datasetPath": "/workspace/dataset.jsonl",
    "rank": $RANK,
    "alpha": $ALPHA,
    "epochs": $EPOCHS,
    "learningRate": 0.0002,
    "batchSize": $BATCH_SIZE,
    "quantize": true,
    "quantizeBits": $QUANTIZE_BITS,
    "outputDir": "/workspace/output",
    "resumeFromCheckpoint": null
}
EOF
fi

echo ""
echo "================================================================"
echo "  RunPod Gate Gradient Capture Pipeline"
echo "================================================================"
echo "  Model:    $MODEL"
echo "  GPU:      $GPU_TYPE_ID"
echo "  QLoRA:    ${QUANTIZE_BITS}-bit"
echo "  Dataset:  $EXAMPLE_COUNT coding examples"
echo "  Epochs:   $EPOCHS"
echo "  Output:   $OUTPUT_DIR"
echo "  Budget:   ~\$0.33/hr (A6000) or ~\$1.19/hr (A100 for 32B+)"
if [[ -n "$RESUME_DIR" ]]; then
    echo "  Mode:     RESUME (from checkpoint: $(load_checkpoint))"
fi
echo "================================================================"
echo ""

# ── Create or reattach pod ───────────────────────────────────────────
if ! phase_complete "pod_created"; then
    echo "Creating RunPod pod..."

    POD_RESPONSE=$(create_pod)
    POD_ID=$(echo "$POD_RESPONSE" | jq -r '.data.podFindAndDeployOnDemand.id // empty')

    if [[ -z "$POD_ID" ]]; then
        echo "Failed to create pod. Response:"
        echo "$POD_RESPONSE" | jq .
        echo ""
        echo "Common issues:"
        echo "  - GPU type unavailable: try a different GPU or SECURE cloud"
        echo "  - Insufficient funds: add credits at runpod.io"
        echo "  - Invalid API key: check RUNPOD_API_KEY in ~/.continuum/config.env"
        exit 1
    fi

    echo "Pod created: $POD_ID"
    echo "$POD_ID" > "$OUTPUT_DIR/pod-id.txt"
    save_checkpoint "pod_created"
else
    POD_ID=$(jq -r '.podId // empty' "$OUTPUT_DIR/checkpoint.json")
    if [[ -z "$POD_ID" && -f "$OUTPUT_DIR/pod-id.txt" ]]; then
        POD_ID=$(cat "$OUTPUT_DIR/pod-id.txt")
    fi

    # Check if pod is still alive — if terminated (credits ran out, preempted, etc.), create a new one
    if [[ -n "$POD_ID" ]]; then
        POD_STATUS=$(query_pod "$POD_ID" | jq -r '.data.pod // empty')
        if [[ -z "$POD_STATUS" || "$POD_STATUS" == "null" ]]; then
            echo "Pod $POD_ID no longer exists (terminated/expired). Creating a new one..."
            POD_RESPONSE=$(create_pod)
            POD_ID=$(echo "$POD_RESPONSE" | jq -r '.data.podFindAndDeployOnDemand.id // empty')
            if [[ -z "$POD_ID" ]]; then
                echo "Failed to create replacement pod:"
                echo "$POD_RESPONSE" | jq .
                exit 1
            fi
            echo "New pod created: $POD_ID"
            echo "$POD_ID" > "$OUTPUT_DIR/pod-id.txt"
            # Reset to pod_created — need to re-wait, re-upload, re-install deps
            # (training output is gone with the old pod)
            # But dataset and config are still local, so dataset_ready is still valid
            save_checkpoint "pod_created"
        else
            echo "Reattaching to pod: $POD_ID"
        fi
    fi
fi

# Set up cleanup trap — only terminates if results already downloaded
trap "cleanup_pod" EXIT

# ── Wait for pod ready ─────────────────────────────────────────────────
if ! phase_complete "pod_running"; then
    wait_for_pod "$POD_ID"
    sleep 10  # Give SSH daemon time to start
    save_checkpoint "pod_running"
else
    echo "Pod already running"
fi

# ── Get SSH connection ─────────────────────────────────────────────────
if [[ -n "$(jq -r '.sshHost // empty' "$OUTPUT_DIR/checkpoint.json" 2>/dev/null)" ]]; then
    SSH_HOST=$(jq -r '.sshHost' "$OUTPUT_DIR/checkpoint.json")
    SSH_PORT=$(jq -r '.sshPort' "$OUTPUT_DIR/checkpoint.json")
else
    echo "Getting SSH connection info..."
    SSH_CONNECT=$(get_ssh_connection "$POD_ID")

    if [[ -z "$SSH_CONNECT" ]]; then
        echo "Could not get SSH connection info. Querying pod..."
        query_pod "$POD_ID" | jq '.data.pod.runtime'
        echo ""
        echo "The pod may still be initializing. Try resuming in a minute:"
        echo "  $0 --resume=$OUTPUT_DIR"
        exit 1
    fi

    SSH_HOST=$(echo "$SSH_CONNECT" | cut -d' ' -f1)
    SSH_PORT=$(echo "$SSH_CONNECT" | cut -d' ' -f2)
    # Re-save checkpoint with SSH info
    save_checkpoint "pod_running"
fi

echo "SSH: root@${SSH_HOST} -p ${SSH_PORT}"

# Use functions instead of string variables to avoid zsh word-splitting issues
run_ssh() {
    ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        -o ConnectTimeout=30 -o ServerAliveInterval=30 \
        -i "$SSH_KEY" -p "$SSH_PORT" "root@$SSH_HOST" "$@"
}
run_scp() {
    scp -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        -i "$SSH_KEY" -P "$SSH_PORT" "$@"
}

# Wait for SSH to be actually ready
echo -n "Waiting for SSH..."
for i in $(seq 1 30); do
    if run_ssh "echo ok" 2>/dev/null | grep -q ok; then
        echo " connected"
        break
    fi
    if [[ $i -eq 30 ]]; then
        echo " FAILED after 150s"
        echo "Try resuming later: $0 --resume=$OUTPUT_DIR"
        exit 1
    fi
    echo -n "."
    sleep 5
done

# ── Upload files ───────────────────────────────────────────────────────
if ! phase_complete "files_uploaded"; then
    echo "Uploading training files..."
    run_scp "$DATASET_PATH" "root@${SSH_HOST}:/workspace/dataset.jsonl"
    run_scp "$CONFIG_PATH" "root@${SSH_HOST}:/workspace/train-config.json"
    run_scp "$SCRIPT_DIR/peft-train.py" "root@${SSH_HOST}:/workspace/peft-train.py"
    echo "Files uploaded"
    save_checkpoint "files_uploaded"
else
    echo "Skipping upload (already done)"
fi

# ── Install deps ──────────────────────────────────────────────────────
if ! phase_complete "deps_installed"; then
    echo "Installing dependencies on pod..."
    run_ssh bash << 'REMOTE_DEPS'
set -euo pipefail
echo "=== RunPod Training Environment ==="
echo "GPU: $(nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null || echo 'N/A')"
echo "Python: $(python3 --version)"
echo "Disk: $(df -h /workspace | tail -1)"

# Upgrade HF libs to latest (image has older versions with QLoRA bugs)
pip install -q --upgrade transformers peft datasets trl accelerate bitsandbytes sentencepiece protobuf 2>&1 | tail -5

python3 -c "import torch; print(f'PyTorch {torch.__version__}, CUDA: {torch.cuda.is_available()}, GPU: {torch.cuda.get_device_name(0) if torch.cuda.is_available() else \"N/A\"}')"
python3 -c "from peft import LoraConfig; print('peft OK')"
python3 -c "from trl import SFTTrainer; print('trl OK')"
python3 -c "import bitsandbytes; print('bitsandbytes OK')"
echo "=== Dependencies ready ==="
REMOTE_DEPS
    save_checkpoint "deps_installed"
else
    echo "Skipping dependency install (already done)"
fi

# ── Run training ──────────────────────────────────────────────────────
if ! phase_complete "training_complete"; then
    echo ""
    echo "Starting LoRA training with gate gradient capture..."
    echo "  Model download: ~15-30 min for 32B, ~5-10 min for 14B"
    echo "  Training: ~10-30 min depending on model size and examples"
    echo ""

    run_ssh bash << 'REMOTE_TRAIN'
set -euo pipefail

mkdir -p /workspace/output

# Check if training already completed (pod may have been left running)
if [[ -f /workspace/output/gate_gradients.json ]]; then
    echo "gate_gradients.json already exists — training was already completed"
    exit 0
fi

echo "Starting training..."
python3 /workspace/peft-train.py \
    --config /workspace/train-config.json \
    --output /workspace/output

echo ""
echo "=== Training Complete ==="

# Verify gate_gradients.json
if [[ -f /workspace/output/gate_gradients.json ]]; then
    echo "gate_gradients.json captured!"
    python3 -c "
import json
with open('/workspace/output/gate_gradients.json') as f:
    data = json.load(f)
scores = data['layer_scores']
print(f'  Layers: {len(scores)}')
print(f'  Heads per layer: {len(scores[0])}')
print(f'  Training steps: {data[\"num_steps\"]}')
print(f'  Model: {data[\"model_name\"]}')
all_scores = [s for layer in scores for s in layer]
print(f'  Score range: {min(all_scores):.4f} - {max(all_scores):.4f}')
print(f'  Mean score: {sum(all_scores)/len(all_scores):.4f}')
dead = sum(1 for s in all_scores if s < 0.1)
dormant = sum(1 for s in all_scores if 0.1 <= s < 0.2)
low = sum(1 for s in all_scores if 0.2 <= s < 0.3)
medium = sum(1 for s in all_scores if 0.3 <= s < 0.5)
active = sum(1 for s in all_scores if 0.5 <= s < 0.7)
hot = sum(1 for s in all_scores if s >= 0.7)
total = len(all_scores)
print(f'  Tier distribution:')
print(f'    Dead (<0.1):       {dead:4d} ({dead/total*100:.1f}%) -> Removed (0 bits)')
print(f'    Dormant (0.1-0.2): {dormant:4d} ({dormant/total*100:.1f}%) -> Ternary (1.58 bits)')
print(f'    Low (0.2-0.3):     {low:4d} ({low/total*100:.1f}%) -> Q2 (2 bits)')
print(f'    Medium (0.3-0.5):  {medium:4d} ({medium/total*100:.1f}%) -> Q4 (4 bits)')
print(f'    Active (0.5-0.7):  {active:4d} ({active/total*100:.1f}%) -> Q8 (8 bits)')
print(f'    Hot (0.7+):        {hot:4d} ({hot/total*100:.1f}%) -> BF16 (16 bits)')
# Estimate memory for Qwen 32B (32 billion params, but only attention heads are affected)
# Attention params ~= 4 * num_layers * num_heads * head_dim * hidden_dim
# Rough: attention is ~1/3 of total params for decoder-only models
attn_fraction = 1/3
total_params_b = 32.5  # Qwen 2.5 Coder 32B
attn_params_b = total_params_b * attn_fraction
non_attn_gb = (total_params_b - attn_params_b) * 2  # BF16 for non-attention
# Per-tier attention memory
tier_bytes = {
    'dead': dead / total * attn_params_b * 1e9 * 0,
    'ternary': dormant / total * attn_params_b * 1e9 * 0.2,
    'q2': low / total * attn_params_b * 1e9 * 0.25,
    'q4': medium / total * attn_params_b * 1e9 * 0.5,
    'q8': active / total * attn_params_b * 1e9 * 1.0,
    'bf16': hot / total * attn_params_b * 1e9 * 2.0,
}
attn_gb = sum(tier_bytes.values()) / 1e9
total_gb = non_attn_gb + attn_gb
print(f'')
print(f'  === Qwen 2.5 Coder 32B Memory Estimate ===')
print(f'  Non-attention (BF16):  {non_attn_gb:.1f} GB')
print(f'  Attention (mixed):     {attn_gb:.1f} GB')
print(f'  TOTAL ESTIMATED:       {total_gb:.1f} GB')
print(f'  (vs {total_params_b * 2:.1f} GB at full BF16)')
print(f'  Reduction: {(1 - total_gb / (total_params_b * 2)) * 100:.1f}%')
"
else
    echo "gate_gradients.json NOT FOUND"
    ls -la /workspace/output/
    exit 1
fi

if [[ -f /workspace/output/training_metrics.json ]]; then
    echo ""
    cat /workspace/output/training_metrics.json
fi
REMOTE_TRAIN

    save_checkpoint "training_complete"
else
    echo "Skipping training (already complete)"
fi

# ── Download results ──────────────────────────────────────────────────
if ! phase_complete "results_downloaded"; then
    echo ""
    echo "Downloading results..."

    mkdir -p "$OUTPUT_DIR/results"
    run_scp "root@${SSH_HOST}:/workspace/output/gate_gradients.json" "$OUTPUT_DIR/results/"
    run_scp "root@${SSH_HOST}:/workspace/output/training_metrics.json" "$OUTPUT_DIR/results/" 2>/dev/null || true
    run_scp "root@${SSH_HOST}:/workspace/output/quantization_info.json" "$OUTPUT_DIR/results/" 2>/dev/null || true

    save_checkpoint "results_downloaded"
else
    echo "Skipping download (already done)"
fi

echo ""
echo "================================================================"
echo "  Gate Gradient Capture Complete!"
echo "================================================================"
echo "  Results: $OUTPUT_DIR/results/"
echo "  gate_gradients.json: $(wc -c < "$OUTPUT_DIR/results/gate_gradients.json" | tr -d ' ') bytes"
echo ""

# ── Local Analysis (no model download needed) ─────────────────────
echo "Running local analysis on gate gradients..."
echo ""

# Quick analysis using Python (no Rust IPC needed)
python3 -c "
import json, sys

with open('$OUTPUT_DIR/results/gate_gradients.json') as f:
    data = json.load(f)

model = data['model_name']
layers = data['layer_scores']
num_heads = data['num_heads']
num_kv_heads = data['num_kv_heads']
num_steps = data['num_steps']

print(f'Model: {model}')
print(f'Layers: {len(layers)}, Heads: {num_heads}, KV Heads: {num_kv_heads}')
print(f'Training steps: {num_steps}')
print()

# Tier thresholds
tiers = {'removed': 0.1, 'ternary': 0.2, 'q2': 0.3, 'q4': 0.5, 'q8': 0.7}

all_scores = [s for layer in layers for s in layer]
tier_counts = {'removed': 0, 'ternary': 0, 'q2': 0, 'q4': 0, 'q8': 0, 'bf16': 0}
for s in all_scores:
    if s < 0.1: tier_counts['removed'] += 1
    elif s < 0.2: tier_counts['ternary'] += 1
    elif s < 0.3: tier_counts['q2'] += 1
    elif s < 0.5: tier_counts['q4'] += 1
    elif s < 0.7: tier_counts['q8'] += 1
    else: tier_counts['bf16'] += 1

total = len(all_scores)
print('Per-head precision assignment:')
for tier, count in tier_counts.items():
    pct = count / total * 100
    bar = '█' * int(pct / 2)
    print(f'  {tier:>8s}: {count:4d} ({pct:5.1f}%) {bar}')

# Estimate memory with mixed precision
head_dim = 64 if num_heads <= 14 else 128
hidden_size = num_heads * head_dim
bytes_per_tier = {'removed': 0, 'ternary': 0.2, 'q2': 0.25, 'q4': 0.5, 'q8': 1.0, 'bf16': 2.0}

# Attention params per head: head_dim * hidden_size * 2 (Q+O) per layer
params_per_head_per_layer = head_dim * hidden_size * 2
total_attn_params = total * params_per_head_per_layer
original_bytes = total_attn_params * 2  # BF16

mixed_bytes = sum(
    tier_counts[t] * params_per_head_per_layer * bytes_per_tier[t]
    for t in tier_counts
)

print()
print(f'Attention parameter savings:')
print(f'  Original (BF16): {original_bytes / 1e9:.2f} GB')
print(f'  Mixed precision: {mixed_bytes / 1e9:.2f} GB')
print(f'  Reduction: {(1 - mixed_bytes / original_bytes) * 100:.1f}%')
print()
print(f'Score range: {min(all_scores):.4f} - {max(all_scores):.4f}')
print(f'Mean score: {sum(all_scores)/len(all_scores):.4f}')
" 2>&1 || echo "(Analysis script failed — results still saved)"

echo ""
echo "================================================================"
echo "  Next: Run compaction on your local machine"
echo "================================================================"
echo ""
echo "  # Analyze (dry run):"
echo "  ./jtag plasticity/analyze --adapterPath=$OUTPUT_DIR/results"
echo ""
echo "  # Full compaction pipeline (download model + compact):"
echo "  ./compact-from-capture.sh $OUTPUT_DIR"
echo ""
echo "  To re-run capture with more epochs or different data:"
echo "    $0 --resume=$OUTPUT_DIR"
echo "    (edit train-config.json first, delete checkpoint.json)"
echo "================================================================"

# Pod cleanup happens via EXIT trap — terminates since results_downloaded
