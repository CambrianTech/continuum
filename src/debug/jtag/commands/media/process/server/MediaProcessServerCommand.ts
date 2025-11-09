/**
 * Media Process Command - Server Implementation
 *
 * Orchestrates ffmpeg execution on server via platform-specific scripts.
 * TypeScript validates parameters and emits events; bash scripts do the work.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { MediaProcessParams, MediaProcessResult, MediaFormat, QualityPreset, VideoCodec, AudioCodec } from '../shared/MediaProcessTypes';
import { createMediaProcessResult, generateOutputPath } from '../shared/MediaProcessTypes';
import { PATHS } from '../../../../system/shared/Constants';
import { Events } from '../../../../system/core/shared/Events';
import { compareVersions, VersionAction, formatActionMessage, type VersionRule } from '../../../../system/shared/VersionComparison';
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';
import { spawn } from 'child_process';

export class MediaProcessServerCommand extends CommandBase<MediaProcessParams, MediaProcessResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('media/process', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<MediaProcessResult> {
    const mediaParams = params as MediaProcessParams;
    const startTime = Date.now();

    // Handle dependency check/install requests
    if (mediaParams.checkDependency) {
      return this.checkFFmpegDependency(mediaParams);
    }

    if (mediaParams.installDependencies) {
      return this.installFFmpegDependency(mediaParams);
    }

    // Validate input file exists
    if (!fs.existsSync(mediaParams.inputPath)) {
      return createMediaProcessResult(mediaParams, {
        success: false,
        error: `Input file not found: ${mediaParams.inputPath}`
      });
    }

    // Generate output path if not provided
    const outputDir = path.resolve(PATHS.MEDIA_OUTPUT);
    fs.mkdirSync(outputDir, { recursive: true });

    const outputPath = mediaParams.outputPath ?? generateOutputPath(
      mediaParams.inputPath,
      mediaParams,
      outputDir
    );

    // Build ffmpeg arguments from parameters
    const ffmpegArgs = this.buildFfmpegArgs(mediaParams, outputPath);

    console.debug(`🎬 MEDIA PROCESS: Running ffmpeg with ${ffmpegArgs.length} arguments`);
    console.debug(`📥 Input: ${mediaParams.inputPath}`);
    console.debug(`📤 Output: ${outputPath}`);

    // Execute ffmpeg and track progress
    try {
      const ffmpegOutput = await this.executeFFmpeg(ffmpegArgs, mediaParams);
      const processingTime = Date.now() - startTime;

      // Get output file stats
      const stats = fs.statSync(outputPath);
      const duration = await this.getMediaDuration(outputPath);

      console.debug(`✅ MEDIA PROCESS: Completed in ${processingTime}ms`);
      console.debug(`📊 Output: ${stats.size} bytes, ${duration?.toFixed(2)}s duration`);

      return createMediaProcessResult(mediaParams, {
        success: true,
        outputPath,
        duration,
        fileSize: stats.size,
        processingTime,
        ffmpegOutput
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ MEDIA PROCESS: Failed -`, errorMessage);

      return createMediaProcessResult(mediaParams, {
        success: false,
        error: `FFmpeg execution failed: ${errorMessage}`
      });
    }
  }

  /**
   * Build ffmpeg command line arguments from MediaProcessParams
   */
  private buildFfmpegArgs(params: MediaProcessParams, outputPath: string): string[] {
    const args: string[] = [];

    // Input file
    args.push('-i', params.inputPath);

    // Speed adjustment (via setpts video filter)
    if (params.speed) {
      args.push('-filter:v', `setpts=${params.speed}*PTS`);
    }

    // Audio operations
    if (params.stripAudio) {
      args.push('-an');
    } else if (params.audioPath) {
      // Replace audio
      args.push('-i', params.audioPath);
      args.push('-c:v', 'copy');
      args.push('-map', '0:v:0');
      args.push('-map', '1:a:0');
    } else if (params.extractAudio) {
      // Audio-only extraction
      args.push('-vn');
    } else if (params.volume) {
      // Volume adjustment
      args.push('-filter:a', `volume=${params.volume}`);
    }

    // Video codec
    if (params.videoCodec && params.videoCodec !== 'copy') {
      const codecMap: Record<VideoCodec, string> = {
        'h264': 'libx264',
        'h265': 'libx265',
        'vp9': 'libvpx-vp9',
        'av1': 'libaom-av1',
        'copy': 'copy'
      };
      args.push('-c:v', codecMap[params.videoCodec]);
    } else if (params.videoCodec === 'copy') {
      args.push('-c:v', 'copy');
    }

    // Audio codec
    if (params.audioCodec && !params.stripAudio) {
      const codecMap: Record<AudioCodec, string> = {
        'aac': 'aac',
        'mp3': 'libmp3lame',
        'opus': 'libopus',
        'flac': 'flac',
        'copy': 'copy'
      };
      args.push('-c:a', codecMap[params.audioCodec]);
    }

    // Quality preset
    if (params.quality) {
      const crfMap: Record<QualityPreset, string> = {
        'low': '28',
        'medium': '23',
        'high': '18',
        'source': '0'
      };
      args.push('-crf', crfMap[params.quality]);
    }

    // Bitrates
    if (params.videoBitrate) {
      args.push('-b:v', params.videoBitrate);
    }
    if (params.audioBitrate && !params.stripAudio) {
      args.push('-b:a', params.audioBitrate);
    }

    // Resolution
    if (params.resolution) {
      args.push('-s', `${params.resolution.width}x${params.resolution.height}`);
    } else if (params.scale) {
      args.push('-vf', `scale=iw*${params.scale}:ih*${params.scale}`);
    }

    // Trimming
    if (params.trim) {
      args.push('-ss', params.trim.start.toString());
      args.push('-to', params.trim.end.toString());
    } else if (params.trimTime) {
      args.push('-ss', params.trimTime.start);
      args.push('-to', params.trimTime.end);
    }

    // Rotation/flipping
    if (params.rotate) {
      const rotationMap: Record<number, string> = {
        90: 'transpose=1',
        180: 'transpose=1,transpose=1',
        270: 'transpose=2'
      };
      args.push('-vf', rotationMap[params.rotate]);
    }
    if (params.flipHorizontal) {
      args.push('-vf', 'hflip');
    }
    if (params.flipVertical) {
      args.push('-vf', 'vflip');
    }

    // GIF-specific options
    if (params.format === 'gif') {
      const fps = params.gifFps ?? 10;
      const maxWidth = params.gifMaxWidth ?? 480;
      args.push('-vf', `fps=${fps},scale=${maxWidth}:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse`);
    }

    // Custom arguments (advanced users)
    if (params.customArgs) {
      args.push(...params.customArgs);
    }

    // Output file (overwrite without prompting)
    args.push('-y', outputPath);

    return args;
  }

  /**
   * Execute ffmpeg command with progress tracking
   */
  private async executeFFmpeg(args: string[], params: MediaProcessParams): Promise<string> {
    return new Promise((resolve, reject) => {
      const ffmpegProcess = spawn('ffmpeg', args);
      let stderrOutput = '';

      ffmpegProcess.stderr.on('data', (data: Buffer) => {
        const line = data.toString();
        stderrOutput += line;

        // Emit progress events if requested
        if (params.emitProgress) {
          const progressMatch = line.match(/time=(\d+):(\d+):(\d+\.\d+)/);
          if (progressMatch) {
            const hours = parseInt(progressMatch[1], 10);
            const minutes = parseInt(progressMatch[2], 10);
            const seconds = parseFloat(progressMatch[3]);
            const timeSeconds = hours * 3600 + minutes * 60 + seconds;

            Events.emit(this.context, 'media:process:progress', {
              inputPath: params.inputPath,
              currentTime: timeSeconds,
              context: params.context,
              sessionId: params.sessionId
            });
          }
        }
      });

      ffmpegProcess.on('close', (code: number | null) => {
        if (code === 0) {
          resolve(stderrOutput);
        } else {
          reject(new Error(`FFmpeg exited with code ${code}\n${stderrOutput}`));
        }
      });

      ffmpegProcess.on('error', (error: Error) => {
        reject(new Error(`FFmpeg spawn error: ${error.message}`));
      });
    });
  }

  /**
   * Get media duration using ffprobe
   */
  private async getMediaDuration(filepath: string): Promise<number | undefined> {
    return new Promise((resolve) => {
      const ffprobe = spawn('ffprobe', [
        '-v', 'error',
        '-show_entries', 'format=duration',
        '-of', 'default=noprint_wrappers=1:nokey=1',
        filepath
      ]);

      let output = '';
      ffprobe.stdout.on('data', (data: Buffer) => {
        output += data.toString();
      });

      ffprobe.on('close', () => {
        const duration = parseFloat(output.trim());
        resolve(isNaN(duration) ? undefined : duration);
      });

      ffprobe.on('error', () => {
        resolve(undefined);
      });
    });
  }

  /**
   * Check if ffmpeg is installed and meets version requirement
   */
  private async checkFFmpegDependency(params: MediaProcessParams): Promise<MediaProcessResult> {
    return new Promise((resolve) => {
      const checkProcess = spawn('ffmpeg', ['-version']);
      let output = '';

      checkProcess.stdout.on('data', (data: Buffer) => {
        output += data.toString();
      });

      checkProcess.on('close', (code: number | null) => {
        if (code !== 0) {
          resolve(createMediaProcessResult(params, {
            success: true,
            dependencyInstalled: false
          }));
          return;
        }

        // Parse version from output (e.g., "ffmpeg version 4.4.2-0ubuntu0.22.04.1")
        const versionMatch = output.match(/ffmpeg version (\d+\.\d+(?:\.\d+)?)/);
        const installedVersion = versionMatch ? versionMatch[1] : null;

        // Default requirement: >= 4.0 (if not specified in params)
        const versionRule: VersionRule = params.minVersion ?? '>=4.0';

        const action = compareVersions(installedVersion, versionRule);
        const message = formatActionMessage(action, installedVersion ?? 'unknown', versionRule);

        console.log(`📦 ffmpeg version check: ${message}`);

        // Determine result based on action
        const needsAction = action === VersionAction.INSTALL || action === VersionAction.UPGRADE;

        resolve(createMediaProcessResult(params, {
          success: true,
          dependencyInstalled: !needsAction,
          warnings: needsAction ? [message] : undefined
        }));
      });

      checkProcess.on('error', () => {
        resolve(createMediaProcessResult(params, {
          success: true,
          dependencyInstalled: false
        }));
      });
    });
  }

  /**
   * Install ffmpeg via platform-specific package manager
   */
  private async installFFmpegDependency(params: MediaProcessParams): Promise<MediaProcessResult> {
    const platform = os.platform();

    if (platform !== 'darwin' && platform !== 'linux') {
      return createMediaProcessResult(params, {
        success: false,
        error: `Unsupported platform for auto-install: ${platform}`
      });
    }

    const installCommand = platform === 'darwin' ? 'brew' : 'apt-get';
    const installArgs = platform === 'darwin'
      ? ['install', 'ffmpeg']
      : ['install', '-y', 'ffmpeg'];

    console.log(`📦 Installing ffmpeg via ${installCommand}...`);

    return new Promise((resolve) => {
      const installProcess = spawn(installCommand, installArgs);
      let output = '';

      installProcess.stdout.on('data', (data: Buffer) => {
        output += data.toString();
        console.log(data.toString());
      });

      installProcess.stderr.on('data', (data: Buffer) => {
        output += data.toString();
        console.log(data.toString());
      });

      installProcess.on('close', (code: number | null) => {
        if (code === 0) {
          console.log(`✅ ffmpeg installed successfully`);
          resolve(createMediaProcessResult(params, {
            success: true,
            dependencyInstalled: true,
            ffmpegOutput: output
          }));
        } else {
          resolve(createMediaProcessResult(params, {
            success: false,
            error: `Installation failed with code ${code}`,
            ffmpegOutput: output
          }));
        }
      });

      installProcess.on('error', (error: Error) => {
        resolve(createMediaProcessResult(params, {
          success: false,
          error: `Installation error: ${error.message}`
        }));
      });
    });
  }
}
