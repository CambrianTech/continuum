# sentinel/run

Run Sentinels from JSON configuration. Allows AIs to create and execute autonomous task pipelines.

## Sentinel Types

| Type | AI Required | Description |
|------|-------------|-------------|
| `build` | No | Agentic compilation with auto-fix |
| `orchestrate` | Yes | LLM-powered planning & execution |
| `screenshot` | No | Take screenshots via Puppeteer |
| `task` | No | Execute a list of tasks |

## Usage

### Build Sentinel (Compile & Fix)

```bash
./jtag sentinel/run --type=build --command="npm run build:ts" --maxAttempts=5 --canAutoFix=true
```

### Orchestrate Sentinel (LLM-Powered)

```bash
# With capacity (power level)
./jtag sentinel/run --type=orchestrate \
  --goal="Create a hello world HTML file at output/index.html" \
  --capacity=small

# With specific provider
./jtag sentinel/run --type=orchestrate \
  --goal="Create a counter app" \
  --provider=anthropic \
  --modelName="claude-3-5-sonnet-20241022"

# Capacity levels: tiny, small, medium, large, sota
# Providers: local, ollama, anthropic, openai, auto
```

### Screenshot Sentinel

```bash
# Screenshot a file
./jtag sentinel/run --type=screenshot \
  --target="./output/index.html" \
  --filename="preview.png" \
  --outputDir="/tmp/screenshots"

# Screenshot a URL
./jtag sentinel/run --type=screenshot \
  --target="https://example.com" \
  --filename="example.png"
```

### Task Sentinel

```bash
./jtag sentinel/run --type=task --tasks='[
  {"name": "Create HTML", "action": "write", "file": "index.html", "content": "<h1>Hello</h1>"},
  {"name": "Build", "action": "build", "command": "npm run build"},
  {"name": "Screenshot", "action": "run", "command": "./jtag interface/screenshot"}
]'
```

## Async Mode (Handles)

By default, long operations run async and return a handle:

```bash
# Start async (returns handle immediately)
./jtag sentinel/run --type=orchestrate --goal="Create snake game" --async=true

# Response: { "handle": "abc12345", "completed": false }

# Check status
./jtag sentinel/status --handle=abc12345
```

## Events

Subscribe to progress events:

- `sentinel:progress` - Step updates with progress percentage
- `sentinel:complete` - Sentinel finished
- `sentinel:error` - Sentinel failed
- `sentinel:file:created` - File was created
- `sentinel:screenshot` - Screenshot was taken

## Examples

### AI Creating a Web App

```json
{
  "type": "orchestrate",
  "goal": "Create a todo list web app with HTML, CSS, and JavaScript at apps/todo/",
  "capacity": "medium",
  "maxIterations": 10,
  "screenshotDir": "/tmp/previews"
}
```

### AI Running Build Pipeline

```json
{
  "type": "task",
  "tasks": [
    {"name": "Install deps", "action": "run", "command": "npm install"},
    {"name": "Build", "action": "build", "command": "npm run build"},
    {"name": "Test", "action": "run", "command": "npm test"}
  ]
}
```

### AI Taking Screenshots for QA

```json
{
  "type": "screenshot",
  "target": "http://localhost:3000",
  "filename": "app-state.png",
  "viewport": {"width": 1280, "height": 720}
}
```
