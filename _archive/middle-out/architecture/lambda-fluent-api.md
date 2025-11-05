# Distributed Command Execution

## 🌊 Elegant Promise Chaining

Commands are autonomous and figure out their own execution using daemons. Remote execution is just another context that commands can use.

### 🎯 Simple Command Chaining

```typescript
// Commands figure out their own execution needs
await continuum.screenshot({selector: 'chat-widget'});  // → browser daemon

// Remote sets execution context, then chain commands
await continuum.remote({peer: 'video-peer'}).runway({
  prompt: 'cat playing piano',
  duration: 10
}).resize({width: 1920, height: 1080}).save({to: 'session'});

// Each command decides where to run based on context and capabilities
await continuum.lambda({region: 'us-west-2'}).screenshot({selector: 'body'});
```

### 🔍 Discoverable Command Marketplace

The P2P mesh becomes a living command marketplace where capabilities are discoverable:

```typescript
// Discover what remote peers can do
const videoPeerCommands = await continuum.remote({peer: 'video-peer'}).list();
// Returns: {runway, ffmpeg, blender, gpu_status, ...}

// Search for capabilities across the mesh
const videoCommands = await continuum.mesh().search('video');
const aiCommands = await continuum.mesh().search('ai');
const gpuPeers = await continuum.mesh().search({hardware: 'gpu'});

// Get help for any command
const runwayHelp = await continuum.remote({peer: 'video-peer'}).help('runway');
// Returns examples, parameters, and usage documentation

// Auto-route to best peer for a task
await continuum.mesh().auto({task: 'video-generation'})
  .runway({prompt: 'dancing robot'})
  .save({to: 'session'});
```

Each command intelligently decides its execution path:

```typescript
// Commands analyze their needs and choose execution strategy
class ScreenshotCommand {
  async execute(context: ExecutionContext) {
    // Analyzes: selector needs DOM access → must run in browser
    // Checks: remote context set → coordinate with remote daemon
    // Decides: send to remote peer's browser daemon
    
    if (context.remote) {
      return this.executeRemote(context);
    }
    return this.executeLocal(context);
  }
}

class RunwayCommand {
  async execute(context: ExecutionContext) {
    // Analyzes: needs GPU + API keys → find capable peer
    // Checks: large video files → minimize transfers
    // Decides: stay on remote peer with GPU access
    
    const capabilities = await this.findPeerWithCapabilities(['gpu', 'runway-api']);
    return this.executeOn(capabilities.bestPeer);
  }
}
```

### 🚀 Complex Pipeline Example

```typescript
// Multi-peer video generation pipeline
const videoResult = await continuum
  .remote({peer: 'video-generation-peer'})
  .runway({
    prompt: 'cat playing piano in cozy room',
    duration: 10,
    style: 'cinematic'
  })
  .resize({width: 1920, height: 1080})
  .addAudio({track: 'background-piano.mp3'})
  .save({to: 'session'});

// Each command figures out:
// - runway(): Uses remote peer's GPU + API access
// - resize(): Stays on remote peer (avoid large transfers)
// - addAudio(): Transfers audio file to remote peer
// - save(): Transfers final video back to local session
```

### 🏛️ Architecture Benefits

#### Universal Command Interface
Commands work the same locally or distributed:

```typescript
// Same API, different execution contexts
await continuum.screenshot({selector: 'body'});           // Local
await continuum.remote({peer: 'x'}).screenshot({...});    // Remote
await continuum.lambda().screenshot({...});               // Serverless
await continuum.mesh().auto().screenshot({...});          // Auto-routed
```

#### Autonomous Execution
- **Commands are self-aware** - they analyze their requirements
- **Daemons handle coordination** - no complex orchestration needed  
- **Context flows naturally** - remote/lambda/p2p just set execution environment
- **Capabilities are discoverable** - mesh becomes searchable command marketplace

### 🔮 The Vision

The P2P mesh becomes a **shared computer** where:
- **Every peer contributes** their unique capabilities
- **Every command is discoverable** and usable by anyone
- **Every AI/user can leverage** the collective power
- **Every capability is searchable** and documented

Simple promise chaining unlocks distributed computing for everyone.