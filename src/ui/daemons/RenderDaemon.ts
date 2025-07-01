/**
 * RenderDaemon - Package-Driven Asset Processing in Web Worker
 * 
 * ARCHITECTURE:
 * - Packages declare their asset processing needs in package.json
 * - RenderDaemon executes the declared pipeline operations
 * - Runs in Web Worker to avoid blocking main thread
 * - Asset agnostic - handles any asset type via package instructions
 * - Extends BaseDaemon for universal daemon patterns
 */

import { BaseDaemon, DaemonRequest } from './BaseDaemon.js';

export interface AssetPipeline {
  [assetPath: string]: string[]; // asset -> operations array
}

export interface PackageConfig {
  name: string;
  files: string[];
  assetPipeline?: AssetPipeline;
  outputFormats?: { [assetType: string]: string };
}

export interface RenderRequest extends DaemonRequest {
  type: 'render:package' | 'render:asset';
  packagePath?: string;
  packageConfig?: PackageConfig;
  assetUrl?: string;
  operations?: string[];
  outputFormat?: string;
}

export interface ProcessResult {
  success?: boolean;
  assets?: { [assetPath: string]: any };
  asset?: any;
  metadata?: any;
  error?: string;
}

/**
 * RenderDaemon - Generic Package-Driven Asset Processor
 * Extends BaseDaemon for universal daemon lifecycle and communication
 */
export class RenderDaemon extends BaseDaemon {
  private operationPlugins: Map<string, Function> = new Map();
  
  constructor() {
    super('RenderDaemon');
    this.version = '1.0.0';
  }

  protected async onStart(): Promise<void> {
    this.log('Initializing RenderDaemon...');
    
    // Register request handlers
    this.registerHandlers({
      'render:package': this.handlePackageRequest.bind(this),
      'render:asset': this.handleAssetRequest.bind(this)
    });
    
    // Load operation plugins
    this.loadOperationPlugins();
    
    this.log('RenderDaemon initialized with asset-agnostic processing pipeline');
  }

  protected async onStop(): Promise<void> {
    this.log('Cleaning up RenderDaemon...');
    this.operationPlugins.clear();
    this.log('RenderDaemon cleanup complete');
  }

  protected async onHealthCheck(): Promise<boolean> {
    // Check if core operations are available
    const coreOps = ['minify', 'compress', 'optimize'];
    return coreOps.every(op => this.operationPlugins.has(op));
  }

  /**
   * Handle package processing request
   */
  private async handlePackageRequest(request: RenderRequest): Promise<ProcessResult> {
    return this.processPackage(request);
  }

  /**
   * Handle single asset processing request
   */
  private async handleAssetRequest(request: RenderRequest): Promise<ProcessResult> {
    return this.processAsset(request);
  }

  /**
   * Process entire package based on its declared asset pipeline
   */
  private async processPackage(request: RenderRequest): Promise<ProcessResult> {
    let packageConfig: PackageConfig;

    if (request.packageConfig) {
      packageConfig = request.packageConfig;
    } else if (request.packagePath) {
      packageConfig = await this.loadPackageConfig(request.packagePath);
    } else {
      throw new Error('Must provide packageConfig or packagePath');
    }

    const { assetPipeline = {}, outputFormats = {} } = packageConfig;
    const results: { [assetPath: string]: any } = {};

    // Process each asset according to package instructions
    for (const [assetPath, operations] of Object.entries(assetPipeline)) {
      const assetType = this.getAssetType(assetPath);
      const outputFormat = outputFormats[assetType] || 'auto';

      try {
        const assetResult = await this.processAsset({
          type: 'render:asset',
          assetUrl: assetPath,
          operations,
          outputFormat
        });

        if (assetResult.success) {
          results[assetPath] = assetResult.asset;
        } else {
          console.warn(`Failed to process asset ${assetPath}: ${assetResult.error}`);
          results[assetPath] = null;
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.warn(`Error processing asset ${assetPath}: ${errorMessage}`);
        results[assetPath] = null;
      }
    }

    return {
      success: true,
      assets: results,
      metadata: {
        packageName: packageConfig.name,
        processedAssets: Object.keys(results).length,
        timestamp: Date.now()
      }
    };
  }

  /**
   * Process single asset with specified operations
   */
  private async processAsset(request: RenderRequest): Promise<ProcessResult> {
    if (!request.assetUrl || !request.operations) {
      throw new Error('Asset processing requires assetUrl and operations');
    }

    try {
      // Load the asset
      let asset = await this.loadAsset(request.assetUrl);

      // Apply operations in sequence
      for (const operation of request.operations) {
        asset = await this.applyOperation(asset, operation);
      }

      // Format output
      const formattedAsset = await this.formatOutput(asset, request.outputFormat || 'auto');

      return {
        success: true,
        asset: formattedAsset,
        metadata: {
          assetUrl: request.assetUrl,
          operations: request.operations,
          outputFormat: request.outputFormat,
          size: this.getAssetSize(formattedAsset),
          timestamp: Date.now()
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Load package.json configuration
   */
  private async loadPackageConfig(packagePath: string): Promise<PackageConfig> {
    const packageJsonUrl = `${packagePath}/package.json`;
    const response = await fetch(packageJsonUrl);
    
    if (!response.ok) {
      throw new Error(`Failed to load package.json from ${packageJsonUrl}: ${response.status}`);
    }

    return await response.json();
  }

  /**
   * Load asset from URL
   */
  private async loadAsset(assetUrl: string): Promise<any> {
    const response = await fetch(assetUrl);
    
    if (!response.ok) {
      throw new Error(`Failed to load asset from ${assetUrl}: ${response.status}`);
    }

    const contentType = response.headers.get('content-type') || '';
    
    // Return appropriate format based on content type
    if (contentType.includes('text/') || contentType.includes('application/json')) {
      return await response.text();
    } else if (contentType.includes('image/') || contentType.includes('video/') || contentType.includes('audio/')) {
      return await response.blob();
    } else {
      return await response.arrayBuffer();
    }
  }

  /**
   * Apply single operation to asset
   */
  private async applyOperation(asset: any, operation: string): Promise<any> {
    const [opName, ...params] = operation.split(':');
    const operationFn = this.operationPlugins.get(opName);

    if (!operationFn) {
      console.warn(`Unknown operation: ${opName}, skipping...`);
      return asset;
    }

    return await operationFn(asset, ...params);
  }

  /**
   * Format output according to specified format
   */
  private async formatOutput(asset: any, format: string): Promise<any> {
    switch (format) {
      case 'string':
        return String(asset);
      case 'blob':
        return asset instanceof Blob ? asset : new Blob([asset]);
      case 'arraybuffer':
        return asset instanceof ArrayBuffer ? asset : new ArrayBuffer(0);
      case 'auto':
      default:
        return asset;
    }
  }

  /**
   * Get asset type from file path
   */
  private getAssetType(assetPath: string): string {
    const ext = assetPath.split('.').pop()?.toLowerCase() || '';
    
    if (['css', 'scss', 'sass', 'less'].includes(ext)) return 'css';
    if (['js', 'ts', 'jsx', 'tsx'].includes(ext)) return 'js';
    if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp'].includes(ext)) return 'image';
    if (['mp4', 'webm', 'avi', 'mov'].includes(ext)) return 'video';
    if (['mp3', 'wav', 'ogg', 'flac'].includes(ext)) return 'audio';
    if (['gltf', 'glb', 'obj', 'fbx'].includes(ext)) return '3d';
    if (['html', 'htm'].includes(ext)) return 'html';
    
    return 'unknown';
  }

  /**
   * Get asset size for metadata
   */
  private getAssetSize(asset: any): number {
    if (typeof asset === 'string') return asset.length;
    if (asset instanceof Blob) return asset.size;
    if (asset instanceof ArrayBuffer) return asset.byteLength;
    return 0;
  }

  /**
   * Load operation plugins
   */
  private loadOperationPlugins(): void {
    // Basic operations - can be extended with more sophisticated plugins
    this.operationPlugins.set('minify', this.minifyOperation.bind(this));
    this.operationPlugins.set('compress', this.compressOperation.bind(this));
    this.operationPlugins.set('resize', this.resizeOperation.bind(this));
    this.operationPlugins.set('optimize', this.optimizeOperation.bind(this));
    this.operationPlugins.set('bundle', this.bundleOperation.bind(this));
  }

  /**
   * Basic operation implementations
   */
  private async minifyOperation(asset: any): Promise<any> {
    if (typeof asset === 'string') {
      // Simple CSS/JS minification
      return asset
        .replace(/\/\*[\s\S]*?\*\//g, '') // Remove comments
        .replace(/\s+/g, ' ')              // Collapse whitespace
        .trim();
    }
    return asset;
  }

  private async compressOperation(asset: any, format?: string): Promise<any> {
    // Placeholder for compression logic
    console.log(`Compressing asset to ${format || 'default'}`);
    return asset;
  }

  private async resizeOperation(asset: any, dimensions?: string): Promise<any> {
    // Placeholder for resize logic
    console.log(`Resizing asset to ${dimensions || 'default'}`);
    return asset;
  }

  private async optimizeOperation(asset: any): Promise<any> {
    // Placeholder for optimization logic
    console.log('Optimizing asset');
    return asset;
  }

  private async bundleOperation(asset: any): Promise<any> {
    // Placeholder for bundling logic
    console.log('Bundling asset');
    return asset;
  }
}

// Initialize daemon when loaded in Web Worker
if (typeof self !== 'undefined' && typeof window === 'undefined') {
  new RenderDaemon(); // Auto-start daemon in Web Worker context
  // BaseDaemon handles auto-start in Web Worker context
}