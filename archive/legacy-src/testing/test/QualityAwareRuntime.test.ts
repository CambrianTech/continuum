/**
 * Tests for Quality-Aware Runtime System
 */

import { describe, it, expect } from '@jest/globals';
import { QualityAwareRuntime } from '../QualityAwareRuntime.js';
import { ModuleGraduationStatus } from '../../types/ModuleQualitySchema.js';

describe('QualityAwareRuntime', () => {
  const runtime = new QualityAwareRuntime();

  describe('Quality Ranking', () => {
    it('should rank PERFECT higher than GRADUATED', () => {
      const perfectRank = runtime.getQualityRank(ModuleGraduationStatus.PERFECT);
      const graduatedRank = runtime.getQualityRank(ModuleGraduationStatus.GRADUATED);
      
      expect(perfectRank).toBeGreaterThan(graduatedRank);
    });

    it('should rank GRADUATED higher than CANDIDATE', () => {
      const graduatedRank = runtime.getQualityRank(ModuleGraduationStatus.GRADUATED);
      const candidateRank = runtime.getQualityRank(ModuleGraduationStatus.CANDIDATE);
      
      expect(graduatedRank).toBeGreaterThan(candidateRank);
    });

    it('should rank BROKEN lowest', () => {
      const brokenRank = runtime.getQualityRank(ModuleGraduationStatus.BROKEN);
      const unknownRank = runtime.getQualityRank(ModuleGraduationStatus.UNKNOWN);
      
      expect(brokenRank).toBe(0);
      expect(brokenRank).toBeLessThan(unknownRank);
    });
  });

  describe('Quality-Aware Execution', () => {
    it('should execute perfect modules normally', async () => {
      const mockModule = {
        name: 'perfect-module',
        path: '/test/path',
        packageJson: {
          name: 'perfect-module',
          version: '1.0.0',
          continuum: {
            type: 'daemon' as const,
            quality: {
              status: ModuleGraduationStatus.PERFECT,
              verification: {
                lastChecked: new Date().toISOString(),
                verifiedStatus: ModuleGraduationStatus.PERFECT,
                qualityScore: 100,
                issues: {
                  eslint: 0,
                  typescript: 0,
                  tests: 0,
                  compliance: 0
                },
                systemManaged: true as const
              }
            }
          }
        },
        verification: {
          lastChecked: new Date().toISOString(),
          verifiedStatus: ModuleGraduationStatus.PERFECT,
          qualityScore: 100,
          issues: {
            eslint: 0,
            typescript: 0,
            tests: 0,
            compliance: 0
          },
          systemManaged: true as const
        },
        qualityRank: 100
      };

      const mockExecute = jest.fn().mockResolvedValue('success');
      const result = await runtime.executeWithQualityContext(mockModule, mockExecute);
      
      expect(mockExecute).toHaveBeenCalled();
      expect(result).toBe('success');
    });

    it('should refuse to execute broken modules', async () => {
      const mockModule = {
        name: 'broken-module',
        path: '/test/path',
        packageJson: {
          name: 'broken-module',
          version: '1.0.0',
          continuum: {
            type: 'daemon' as const,
            quality: {
              status: ModuleGraduationStatus.BROKEN,
              verification: {
                lastChecked: new Date().toISOString(),
                verifiedStatus: ModuleGraduationStatus.BROKEN,
                qualityScore: 0,
                issues: {
                  eslint: 50,
                  typescript: 10,
                  tests: 5,
                  compliance: 3
                },
                degradationReasons: ['Critical compilation errors', 'All tests failing'],
                systemManaged: true as const
              }
            }
          }
        },
        verification: {
          lastChecked: new Date().toISOString(),
          verifiedStatus: ModuleGraduationStatus.BROKEN,
          qualityScore: 0,
          issues: {
            eslint: 50,
            typescript: 10,
            tests: 5,
            compliance: 3
          },
          degradationReasons: ['Critical compilation errors', 'All tests failing'],
          systemManaged: true as const
        },
        qualityRank: 0
      };

      const mockExecute = jest.fn();
      
      await expect(
        runtime.executeWithQualityContext(mockModule, mockExecute)
      ).rejects.toThrow('broken and cannot be executed');
      
      expect(mockExecute).not.toHaveBeenCalled();
    });
  });

  describe('Quality Filtering', () => {
    it('should validate quality filter behavior', () => {
      // Test that quality ranking works as expected
      const statuses = [
        ModuleGraduationStatus.PERFECT,
        ModuleGraduationStatus.GRADUATED, 
        ModuleGraduationStatus.CANDIDATE,
        ModuleGraduationStatus.WHITELISTED,
        ModuleGraduationStatus.DEGRADED,
        ModuleGraduationStatus.UNKNOWN,
        ModuleGraduationStatus.BROKEN
      ];

      const ranks = statuses.map(status => runtime.getQualityRank(status));
      
      // Verify descending order
      for (let i = 0; i < ranks.length - 1; i++) {
        expect(ranks[i]).toBeGreaterThan(ranks[i + 1]);
      }
    });
  });
});