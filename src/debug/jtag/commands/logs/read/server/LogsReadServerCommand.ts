import { LogsReadCommand } from '../shared/LogsReadCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  LogsReadParams,
  LogsReadResult,
  LogLine,
  LogStructureAnalysis,
  TemporalView,
  TemporalSegment,
  SeverityView,
  SeverityLevel,
  SpatialView,
  SpatialComponent
} from '../shared/LogsReadTypes';
import { LogFileRegistry } from '../../../../system/core/logging/LogFileRegistry';
import { LogReader } from '../../../../system/core/logging/LogReader';
import { logNameToPath } from '../../shared/LogsShared';
import * as fs from 'fs/promises';

interface CachedAnalysis {
  mtime: number;
  analysis: LogStructureAnalysis;
}

export class LogsReadServerCommand extends LogsReadCommand {
  private registry = new LogFileRegistry();
  private reader = new LogReader();
  private structureCache = new Map<string, CachedAnalysis>();

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logs/read', context, subpath, commander);
  }

  async execute(params: LogsReadParams): Promise<LogsReadResult> {
    const allLogs = await this.registry.discover();
    const filePath = logNameToPath(params.log, allLogs);
    if (!filePath) {
      const { pathToLogName } = await import('../../shared/LogsShared');
      // Filter out session logs by default to keep error message concise
      // Use --includeSessionLogs=true to see all logs
      let logNames = allLogs.map(l => pathToLogName(l.filePath));
      if (!params.includeSessionLogs) {
        logNames = logNames.filter(name => !name.startsWith('session/'));
      }
      const availableLogs = logNames.join(', ');
      const hint = params.includeSessionLogs ? '' : ' (use --includeSessionLogs=true to see session logs)';
      throw new Error(`Log not found: ${params.log}. Use logs/list to see available logs. Available: ${availableLogs}${hint}`);
    }

    // If analyzeStructure flag is set, return structure analysis instead of lines
    if (params.analyzeStructure) {
      const result = await this.reader.readAll(filePath);

      // Convert LogReader.LogLine (Date timestamps) to our LogLine type (string timestamps)
      const convertedLines: LogLine[] = result.lines.map(l => ({
        ...l,
        timestamp: l.timestamp?.toISOString()
      }));

      const structure = await this.analyzeStructure(filePath, convertedLines);

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        log: params.log,
        lines: [],  // Empty when analyzing structure
        totalLines: result.totalLines,
        hasMore: false,
        structure
      };
    }

    let result;
    if (params.tail) {
      result = await this.reader.tail(filePath, params.tail);
    } else if (params.startLine && params.endLine) {
      result = await this.reader.read(filePath, params.startLine, params.endLine);
    } else {
      result = await this.reader.readAll(filePath);
    }

    let lines = result.lines;
    if (params.level) lines = lines.filter(l => l.level === params.level);
    if (params.component) lines = lines.filter(l => l.component === params.component);

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      log: params.log,
      lines: lines.map(l => ({...l, timestamp: l.timestamp?.toISOString()})),
      totalLines: result.totalLines,
      hasMore: result.hasMore,
      nextLine: result.nextOffset
    };
  }

  /**
   * Analyze log structure with caching
   */
  private async analyzeStructure(
    filePath: string,
    lines: LogLine[]
  ): Promise<LogStructureAnalysis> {
    // Check cache first
    const stats = await fs.stat(filePath);
    const cacheKey = `${filePath}:${stats.mtimeMs}`;
    const cached = this.structureCache.get(cacheKey);
    if (cached && cached.mtime === stats.mtimeMs) {
      return cached.analysis;
    }

    // Analyze all three views
    const temporal = this.analyzeTemporalView(lines);
    const severity = this.analyzeSeverityView(lines);
    const spatial = this.analyzeSpatialView(lines);

    // Extract time range
    const timestamps = lines
      .map(l => l.timestamp)
      .filter((t): t is string => t !== undefined && t !== null);

    const timeRange: [string, string] | undefined = timestamps.length > 0
      ? [timestamps[0], timestamps[timestamps.length - 1]]
      : undefined;

    // Build result
    const analysis: LogStructureAnalysis = {
      temporal,
      severity,
      spatial,
      totalLines: lines.length,
      timeRange
    };

    // Cache it
    this.structureCache.set(cacheKey, {
      mtime: stats.mtimeMs,
      analysis
    });

    return analysis;
  }

  /**
   * Temporal View: Group log lines into 15-minute time segments
   */
  private analyzeTemporalView(lines: LogLine[]): TemporalView {
    const segments: TemporalSegment[] = [];

    if (lines.length === 0) {
      return { view: 'temporal', segments: [] };
    }

    // Filter lines with timestamps
    const linesWithTimestamps = lines.filter(l => l.timestamp);
    if (linesWithTimestamps.length === 0) {
      return { view: 'temporal', segments: [] };
    }

    // Group by 15-minute intervals
    const INTERVAL_MS = 15 * 60 * 1000; // 15 minutes
    let currentSegment: TemporalSegment | null = null;

    for (let i = 0; i < linesWithTimestamps.length; i++) {
      const line = linesWithTimestamps[i];
      const timestamp = line.timestamp!;
      const time = new Date(timestamp).getTime();

      if (!currentSegment) {
        // Start first segment
        currentSegment = {
          start: timestamp,
          end: timestamp,
          lines: [line.lineNumber, line.lineNumber],
          eventCount: 1
        };
      } else {
        const segmentStart = new Date(currentSegment.start).getTime();

        // Check if this line fits in current segment (within 15 min of segment start)
        if (time - segmentStart <= INTERVAL_MS) {
          // Extend current segment
          currentSegment.end = timestamp;
          currentSegment.lines[1] = line.lineNumber;
          currentSegment.eventCount++;
        } else {
          // Save current segment and start new one
          segments.push(currentSegment);
          currentSegment = {
            start: timestamp,
            end: timestamp,
            lines: [line.lineNumber, line.lineNumber],
            eventCount: 1
          };
        }
      }
    }

    // Don't forget the last segment
    if (currentSegment) {
      segments.push(currentSegment);
    }

    return {
      view: 'temporal',
      segments
    };
  }

  /**
   * Severity View: Group log lines by log level (ERROR, WARN, INFO, DEBUG)
   */
  private analyzeSeverityView(lines: LogLine[]): SeverityView {
    const levelMap = new Map<string, number[]>();

    // Group lines by level
    for (const line of lines) {
      if (line.level) {
        const level = line.level.toUpperCase();
        if (!levelMap.has(level)) {
          levelMap.set(level, []);
        }
        levelMap.get(level)!.push(line.lineNumber);
      }
    }

    // Convert to SeverityLevel array
    const levels: SeverityLevel[] = [];
    const validLevels: Array<'DEBUG' | 'INFO' | 'WARN' | 'ERROR'> = ['ERROR', 'WARN', 'INFO', 'DEBUG'];

    for (const level of validLevels) {
      const lineNumbers = levelMap.get(level) || [];
      if (lineNumbers.length > 0) {
        levels.push({
          level,
          lines: lineNumbers,
          eventCount: lineNumbers.length
        });
      }
    }

    return {
      view: 'severity',
      levels
    };
  }

  /**
   * Spatial View: Group log lines by component/module name
   */
  private analyzeSpatialView(lines: LogLine[]): SpatialView {
    const componentMap = new Map<string, number[]>();

    // Group lines by component
    for (const line of lines) {
      if (line.component) {
        const component = line.component;
        if (!componentMap.has(component)) {
          componentMap.set(component, []);
        }
        componentMap.get(component)!.push(line.lineNumber);
      }
    }

    // Convert to SpatialComponent array, sorted by event count (descending)
    const components: SpatialComponent[] = Array.from(componentMap.entries())
      .map(([component, lineNumbers]) => ({
        component,
        lines: lineNumbers,
        eventCount: lineNumbers.length
      }))
      .sort((a, b) => b.eventCount - a.eventCount);

    return {
      view: 'spatial',
      components
    };
  }
}
