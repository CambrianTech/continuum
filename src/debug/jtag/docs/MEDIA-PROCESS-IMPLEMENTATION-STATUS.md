# Media Process Command - Implementation Status

## Vision

Create a comprehensive `media/process` command that wraps ffmpeg and other media tools with a clean JTAG command interface. This establishes a pattern for **script orchestration** - commands that delegate complex operations to platform-specific scripts while maintaining type safety and event emission.

## Architecture Principles

### 1. **Script Orchestration Pattern** (NEW!)

Commands can delegate to external scripts while maintaining JTAG architecture:

```
┌─────────────────────────────────────────────┐
│  JTAG Command (TypeScript)                  │
│  - Type-safe parameters                     │
│  - Validation                                │
│  - Event emission                            │
│  - Progress tracking                         │
└──────────────┬──────────────────────────────┘
               │
               │ Calls platform-specific script
               │
┌──────────────▼──────────────────────────────┐
│  Platform Script (Bash/Shell)               │
│  commands/media/process/scripts/            │
│    ├── darwin/                               │
│    │   ├── ffmpeg-process.sh                │
│    │   └── check-dependencies.sh            │
│    └── linux/                                │
│        ├── ffmpeg-process.sh                │
│        └── check-dependencies.sh            │
└─────────────────────────────────────────────┘
```

**Key Benefits**:
- **Type Safety**: Parameters validated in TypeScript before script execution
- **Cross-Platform**: Different scripts for darwin/linux but same API
- **Testable**: Can mock script execution in tests
- **Discoverable**: `./jtag media/process --help` shows all options
- **Event-Driven**: Progress events for long operations
- **Composable**: Other commands can call this programmatically

### 2. **Dependency Management Pattern** (NEW!)

Scripts can check/install their own dependencies:

```bash
# commands/media/process/scripts/darwin/check-dependencies.sh
#!/bin/bash
if ! command -v ffmpeg &> /dev/null; then
  echo "ffmpeg not found"
  exit 1
fi
echo "ffmpeg $(ffmpeg -version | head -n1)"
exit 0
```

```bash
# commands/media/process/scripts/darwin/install-dependencies.sh
#!/bin/bash
brew install ffmpeg
```

**Command Usage**:
```bash
# Check if dependencies are installed
./jtag media/process --checkDependency=true

# Install missing dependencies (with user confirmation)
./jtag media/process --installDependencies=true
```

### 3. **Progress Event Pattern**

Long-running operations emit progress events:

```typescript
// In browser or other commands
Events.subscribe('media:processing:progress', (event: MediaProcessProgressEvent) => {
  console.log(`Processing: ${event.percentage}% complete`);
  console.log(`Speed: ${event.speed} fps`);
  console.log(`ETA: ${event.estimatedTimeRemaining}s`);
});

// Server command emits during processing
Events.emit('media:processing:progress', {
  inputPath,
  currentFrame: 150,
  totalFrames: 300,
  percentage: 50,
  speed: 30.5,
  estimatedTimeRemaining: 5
});
```

## Implementation Status

### ✅ Phase 1: Foundation (COMPLETED)

**Files Created**:
- `system/shared/Constants.ts` - Added `MEDIA_OUTPUT` and `MEDIA_TEMP` paths
- `commands/media/process/shared/MediaProcessTypes.ts` (316 lines)
  - Comprehensive parameter types
  - MediaFormat, QualityPreset, VideoCodec, AudioCodec enums
  - MediaProcessParams (all ffmpeg options)
  - MediaProcessResult
  - MediaProcessProgressEvent
  - Helper functions: createMediaProcessResult(), generateOutputPath()

**Capabilities Defined**:
- Speed adjustment (0.25 = 4x faster, 2.0 = 2x slower)
- Audio manipulation (strip, extract, replace, volume)
- Format conversion (mp4, webm, mov, avi, mkv, gif, mp3, wav, flac, aac)
- Quality presets (low, medium, high, source)
- Resolution/scale adjustment
- Trimming (seconds or HH:MM:SS)
- Rotation and flipping
- GIF-specific options (fps, max width)
- Custom ffmpeg arguments
- Dependency checking and installation

### ⏳ Phase 2: Server Command (NEXT)

**File**: `commands/media/process/server/MediaProcessServerCommand.ts`

**Implementation Checklist**:
```typescript
export class MediaProcessServerCommand extends CommandBase<MediaProcessParams, MediaProcessResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('media/process', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<MediaProcessResult> {
    const processParams = params as MediaProcessParams;
    const startTime = Date.now();

    try {
      // 1. Check dependency first if requested
      if (processParams.checkDependency) {
        return await this.checkDependency();
      }

      // 2. Install dependencies if requested (with confirmation)
      if (processParams.installDependencies) {
        return await this.installDependencies();
      }

      // 3. Validate input file exists
      if (!fs.existsSync(processParams.inputPath)) {
        return createMediaProcessResult(processParams, {
          success: false,
          error: `Input file not found: ${processParams.inputPath}`
        });
      }

      // 4. Generate output path if not provided
      const outputPath = processParams.outputPath ||
        generateOutputPath(processParams.inputPath, processParams, PATHS.MEDIA_OUTPUT);

      // 5. Ensure output directory exists
      fs.mkdirSync(path.dirname(outputPath), { recursive: true });

      // 6. Build ffmpeg command
      const ffmpegArgs = this.buildFfmpegArgs(processParams, outputPath);

      // 7. Execute ffmpeg (with progress events if requested)
      const result = await this.executeFFmpeg(ffmpegArgs, processParams);

      return createMediaProcessResult(processParams, {
        success: true,
        outputPath,
        processingTime: Date.now() - startTime,
        ...result
      });

    } catch (error) {
      return createMediaProcessResult(processParams, {
        success: false,
        error: `Processing failed: ${error instanceof Error ? error.message : String(error)}`,
        processingTime: Date.now() - startTime
      });
    }
  }

  /**
   * Check if ffmpeg is installed
   */
  private async checkDependency(): Promise<MediaProcessResult> {
    const platform = os.platform();
    const scriptPath = path.join(__dirname, '../scripts', platform === 'darwin' ? 'darwin' : 'linux', 'check-dependencies.sh');

    try {
      const { stdout, stderr } = await execPromise(`bash "${scriptPath}"`);
      return {
        success: true,
        dependencyInstalled: true,
        ffmpegOutput: stdout.trim()
      };
    } catch (error) {
      return {
        success: false,
        dependencyInstalled: false,
        error: 'ffmpeg not installed'
      };
    }
  }

  /**
   * Install ffmpeg using platform package manager
   */
  private async installDependencies(): Promise<MediaProcessResult> {
    const platform = os.platform();
    const scriptPath = path.join(__dirname, '../scripts', platform === 'darwin' ? 'darwin' : 'linux', 'install-dependencies.sh');

    try {
      const { stdout } = await execPromise(`bash "${scriptPath}"`);
      return {
        success: true,
        dependencyInstalled: true,
        ffmpegOutput: stdout.trim()
      };
    } catch (error) {
      return {
        success: false,
        dependencyInstalled: false,
        error: `Installation failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Build ffmpeg arguments from parameters
   */
  private buildFfmpegArgs(params: MediaProcessParams, outputPath: string): string[] {
    const args: string[] = ['-i', params.inputPath];

    // Speed adjustment
    if (params.speed !== undefined) {
      args.push('-filter:v', `setpts=${params.speed}*PTS`);
      if (!params.stripAudio && !params.extractAudio) {
        args.push('-filter:a', `atempo=${1 / params.speed}`);
      }
    }

    // Audio operations
    if (params.stripAudio) {
      args.push('-an');
    }
    if (params.extractAudio) {
      const audioPath = params.outputPath?.replace(/\.[^.]+$/, '.mp3') ||
        outputPath.replace(/\.[^.]+$/, '.mp3');
      args.push('-vn', '-acodec', 'libmp3lame', audioPath);
      return args; // Extract audio only
    }
    if (params.audioPath) {
      args.push('-i', params.audioPath, '-c:a', params.audioCodec || 'aac', '-map', '0:v:0', '-map', '1:a:0');
    }
    if (params.volume !== undefined) {
      args.push('-filter:a', `volume=${params.volume}`);
    }

    // Resolution/scale
    if (params.resolution) {
      args.push('-vf', `scale=${params.resolution.width}:${params.resolution.height}`);
    } else if (params.scale !== undefined) {
      args.push('-vf', `scale=iw*${params.scale}:ih*${params.scale}`);
    }

    // Trimming
    if (params.trim) {
      args.push('-ss', String(params.trim.start), '-to', String(params.trim.end));
    } else if (params.trimTime) {
      args.push('-ss', params.trimTime.start, '-to', params.trimTime.end);
    }

    // Rotation/flipping
    if (params.rotate) {
      const transposeMap = { 90: '1', 180: '2,transpose=2', 270: '2' };
      args.push('-vf', `transpose=${transposeMap[params.rotate]}`);
    }
    if (params.flipHorizontal) {
      args.push('-vf', 'hflip');
    }
    if (params.flipVertical) {
      args.push('-vf', 'vflip');
    }

    // Codecs and quality
    args.push('-c:v', params.videoCodec || 'libx264');
    if (params.audioCodec && !params.stripAudio) {
      args.push('-c:a', params.audioCodec);
    }
    if (params.videoBitrate) {
      args.push('-b:v', params.videoBitrate);
    }
    if (params.audioBitrate) {
      args.push('-b:a', params.audioBitrate);
    }

    // GIF-specific
    if (params.format === 'gif') {
      args.push('-r', String(params.gifFps || 10));
      if (params.gifMaxWidth) {
        args.push('-vf', `scale=${params.gifMaxWidth}:-1:flags=lanczos`);
      }
    }

    // Custom arguments
    if (params.customArgs) {
      args.push(...params.customArgs);
    }

    // Output (overwrite without prompting)
    args.push('-y', outputPath);

    return args;
  }

  /**
   * Execute ffmpeg with progress tracking
   */
  private async executeFFmpeg(args: string[], params: MediaProcessParams): Promise<Partial<MediaProcessResult>> {
    return new Promise((resolve, reject) => {
      const ffmpeg = spawn('ffmpeg', args);
      let stderr = '';
      let duration: number | undefined;
      let currentFrame = 0;

      ffmpeg.stderr.on('data', (data) => {
        stderr += data.toString();

        // Parse progress from ffmpeg stderr
        if (params.emitProgress) {
          const durationMatch = /Duration: (\d{2}):(\d{2}):(\d{2})/.exec(stderr);
          if (durationMatch && !duration) {
            duration = parseInt(durationMatch[1]) * 3600 +
                      parseInt(durationMatch[2]) * 60 +
                      parseInt(durationMatch[3]);
          }

          const frameMatch = /frame=\s*(\d+)/.exec(stderr);
          if (frameMatch) {
            currentFrame = parseInt(frameMatch[1]);
            const speedMatch = /speed=\s*([\d.]+)x/.exec(stderr);
            const speed = speedMatch ? parseFloat(speedMatch[1]) : undefined;

            Events.emit('media:processing:progress', {
              inputPath: params.inputPath,
              currentFrame,
              percentage: duration ? (currentFrame / duration) * 100 : undefined,
              speed
            } as MediaProcessProgressEvent);
          }
        }
      });

      ffmpeg.on('close', (code) => {
        if (code === 0) {
          const fileSize = fs.statSync(args[args.length - 1]).size;
          resolve({
            fileSize,
            duration,
            ffmpegOutput: stderr
          });
        } else {
          reject(new Error(`ffmpeg exited with code ${code}\n${stderr}`));
        }
      });

      ffmpeg.on('error', reject);
    });
  }
}
```

### ⏳ Phase 3: Platform Scripts

**Checklist**:
- [ ] Create `commands/media/process/scripts/darwin/check-dependencies.sh`
- [ ] Create `commands/media/process/scripts/darwin/install-dependencies.sh`
- [ ] Create `commands/media/process/scripts/darwin/ffmpeg-process.sh` (if direct shell needed)
- [ ] Create `commands/media/process/scripts/linux/check-dependencies.sh`
- [ ] Create `commands/media/process/scripts/linux/install-dependencies.sh`
- [ ] Create `commands/media/process/scripts/linux/ffmpeg-process.sh`
- [ ] Make all scripts executable (chmod +x)

**Example: darwin/check-dependencies.sh**:
```bash
#!/bin/bash
# Check if ffmpeg is installed on macOS

if ! command -v ffmpeg &> /dev/null; then
  echo "ffmpeg not found" >&2
  exit 1
fi

# Output version for confirmation
ffmpeg -version | head -n1
exit 0
```

**Example: darwin/install-dependencies.sh**:
```bash
#!/bin/bash
# Install ffmpeg on macOS using Homebrew

if ! command -v brew &> /dev/null; then
  echo "Homebrew not found. Please install from https://brew.sh" >&2
  exit 1
fi

echo "Installing ffmpeg via Homebrew..."
brew install ffmpeg
exit $?
```

### ⏳ Phase 4: Browser Command

**File**: `commands/media/process/browser/MediaProcessBrowserCommand.ts`

```typescript
export class MediaProcessBrowserCommand extends CommandBase<MediaProcessParams, MediaProcessResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('media/process', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<MediaProcessResult> {
    // Browser simply delegates to server via Commands.execute()
    return await Commands.execute<MediaProcessResult>('media/process', params);
  }
}
```

### ⏳ Phase 5: Testing

**Unit Tests** (`tests/unit/media-process.test.ts`):
```typescript
describe('MediaProcessTypes', () => {
  it('should generate output path with speed suffix', () => {
    const outputPath = generateOutputPath(
      '/path/to/video.mov',
      { inputPath: '/path/to/video.mov', speed: 0.25 },
      '/output'
    );
    expect(outputPath).toBe('/output/video-4.0x.mov');
  });

  it('should detect format from outputPath', () => {
    const outputPath = generateOutputPath(
      '/path/to/video.mov',
      { inputPath: '/path/to/video.mov', outputPath: '/out/video.mp4' },
      '/output'
    );
    expect(outputPath).toContain('.mp4');
  });

  // ... more parameter validation tests
});
```

**Integration Tests** (manual):
```bash
# Test 1: Speed up video
./jtag media/process --inputPath="/path/to/video.mov" --speed=0.25 --stripAudio=true

# Test 2: Convert to GIF
./jtag media/process --inputPath="/path/to/video.mov" --format="gif" --gifFps=10 --gifMaxWidth=480

# Test 3: Extract audio
./jtag media/process --inputPath="/path/to/video.mp4" --extractAudio=true

# Test 4: Trim and compress
./jtag media/process --inputPath="/path/to/long-video.mp4" --trim='{"start":10,"end":60}' --quality="low"

# Test 5: Check dependency
./jtag media/process --checkDependency=true

# Test 6: Install dependency (if ffmpeg not installed)
brew uninstall ffmpeg  # Test fresh install
./jtag media/process --installDependencies=true
```

### ⏳ Phase 6: Documentation

**Update CLAUDE.md**:
```markdown
## 🎬 MEDIA PROCESSING

### media/process - Comprehensive video/audio/image processing

**Command**: `./jtag media/process`

**Use Cases**:
- Speed up/slow down videos
- Convert formats (mp4, webm, gif, etc.)
- Strip/extract/replace audio
- Trim videos
- Create GIFs from videos
- Adjust resolution and quality

**Examples**:
```bash
# Speed up 4x and strip audio (what we just did!)
./jtag media/process --inputPath="/Users/joel/Desktop/ui-example.mov" --speed=0.25 --stripAudio=true

# Convert to optimized GIF
./jtag media/process --inputPath="demo.mp4" --format="gif" --gifMaxWidth=480 --gifFps=10

# Extract audio to MP3
./jtag media/process --inputPath="video.mp4" --extractAudio=true

# Trim and compress
./jtag media/process --inputPath="long-video.mp4" --trim='{"start":10,"end":60}' --quality="medium"
```

**Architecture Pattern**: Script Orchestration
- TypeScript command validates parameters and emits events
- Platform-specific bash scripts handle actual processing
- Supports darwin/linux with different implementations
- Dependencies managed via package managers (brew, apt)
```

## Generalization: Script Orchestration Pattern

This establishes a **reusable pattern** for future commands:

### When to Use Script Orchestration

Use this pattern when a command needs to:
1. **Delegate to external tools** (ffmpeg, imagemagick, git, etc.)
2. **Handle platform differences** (macOS vs Linux vs Windows)
3. **Manage dependencies** (install if missing, check versions)
4. **Run long operations** (need progress tracking)
5. **Execute complex shell pipelines** (easier in bash than TypeScript)

### Template for Future Commands

```
commands/example/
├── shared/
│   └── ExampleTypes.ts          # Type-safe parameters
├── server/
│   └── ExampleServerCommand.ts  # Orchestrates script execution
├── browser/
│   └── ExampleBrowserCommand.ts # Delegates to server
└── scripts/
    ├── darwin/
    │   ├── check-dependencies.sh
    │   ├── install-dependencies.sh
    │   └── process.sh
    └── linux/
        ├── check-dependencies.sh
        ├── install-dependencies.sh
        └── process.sh
```

### Examples of Future Script-Orchestrated Commands

1. **`media/transcode`** - Batch video transcoding
2. **`media/thumbnail`** - Generate video thumbnails
3. **`image/process`** - ImageMagick wrapper (resize, crop, effects)
4. **`git/analyze`** - Complex git log analysis
5. **`system/backup`** - Automated backup via rsync/rclone
6. **`docker/deploy`** - Container deployment workflows
7. **`security/scan`** - Run security scanners (clamav, etc.)

## Next Steps

1. **Complete Phase 2**: Implement MediaProcessServerCommand
2. **Create Platform Scripts**: darwin/linux dependency management
3. **Implement Browser Command**: Simple delegation
4. **Write Tests**: Unit tests for parameter generation
5. **Manual Testing**: Test all capabilities end-to-end
6. **Document Pattern**: Add to ARCHITECTURE-RULES.md
7. **Commit**: Create comprehensive commit message

## Related Documents

- [Storage Adapter Abstraction](./STORAGE-ADAPTER-ABSTRACTION.md) - Similar abstraction pattern
- [CRUD Event Test Architecture](./CRUD-EVENT-TEST-ARCHITECTURE.md) - Event testing patterns
- [Architecture Rules](./ARCHITECTURE-RULES.md) - Core architecture principles
