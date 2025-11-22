# Complexity Detector Refactoring - Making It Plug and Play

**Status**: IN PROGRESS (Phase 2A+ Architectural Fix)
**Issue**: User feedback - "Your code is too flat, specific concerns mixed around"
**Solution**: Separate abstractions using interface pattern (like ORM DataAdapter)

---

## The Problem: Tightly Coupled Implementation

**BEFORE (Flat, Brittle):**
```typescript
// ProgressiveScorer.ts - HARD-CODED to RegExp approach
const INDICATOR_PATTERNS: Record<UpgradeIndicator['type'], RegExp[]> = {
  hedging: [/\b(it depends)\b/i, ...],  // 25 hard-coded patterns
  // ... rest of patterns
};

class ProgressiveScorer {
  analyze(chunk: string, offset: number): ScoringResult {
    // Detection logic MIXED with scoring logic
    const indicators = this.detectIndicators(chunk, offset);
    return this.evaluateUpgrade(indicators);
  }

  private detectIndicators(chunk: string, offset: number): UpgradeIndicator[] {
    // RegExp matching hard-coded here
  }
}
```

**Problems:**
- ❌ Can't swap RegExp for embedding-based detection
- ❌ Can't A/B test different approaches
- ❌ Can't plug in ML classifiers
- ❌ No abstraction layer separating "what" from "how"

---

## The Solution: Interface-Based Architecture

**AFTER (Layered, Extensible):**

### 1. Pure Abstraction Layer
```typescript
// ComplexityDetector.ts - INTERFACE (like DataAdapter)
export interface ComplexityDetector {
  analyze(chunk: string, offset: number): UpgradeIndicator[];
  getName(): string;
}

export class ComplexityDetectorFactory {
  static create(type: 'regex' | 'embedding' | 'ml'): ComplexityDetector {
    // Runtime selection of implementation
  }
}
```

### 2. Concrete Implementations (Plug and Play)
```typescript
// RegexComplexityDetector.ts - ONE approach (not THE approach)
export class RegexComplexityDetector implements ComplexityDetector {
  private patterns: Record<UpgradeIndicator['type'], RegExp[]>;

  analyze(chunk: string, offset: number): UpgradeIndicator[] {
    // Pattern matching implementation
  }

  getName(): string {
    return 'RegexComplexityDetector';
  }
}

// EmbeddingComplexityDetector.ts - FUTURE implementation
export class EmbeddingComplexityDetector implements ComplexityDetector {
  analyze(chunk: string, offset: number): UpgradeIndicator[] {
    // Semantic similarity detection using embeddings
  }

  getName(): string {
    return 'EmbeddingComplexityDetector';
  }
}

// MLComplexityDetector.ts - FUTURE implementation
export class MLComplexityDetector implements ComplexityDetector {
  analyze(chunk: string, offset: number): UpgradeIndicator[] {
    // Trained classifier for detecting complexity
  }

  getName(): string {
    return 'MLComplexityDetector';
  }
}
```

### 3. Consumer Uses Abstraction (Dependency Injection)
```typescript
// ProgressiveScorer.ts - REFACTORED to use abstraction
export class ProgressiveScorer {
  private detector: ComplexityDetector;  // ← Interface, not concrete class
  private config: ProgressiveScorerConfig;
  private indicators: UpgradeIndicator[] = [];
  private tokensAnalyzed: number = 0;

  constructor(
    detector: ComplexityDetector,  // ← Injected (not hard-coded!)
    config?: Partial<ProgressiveScorerConfig>
  ) {
    this.detector = detector;
    this.config = { ...DEFAULT_PROGRESSIVE_SCORER_CONFIG, ...config };
  }

  analyze(chunk: string, offset: number): ScoringResult {
    // Update tokens
    this.tokensAnalyzed += Math.floor(chunk.length / 4);

    // Delegate detection to injected detector
    const newIndicators = this.detector.analyze(chunk, offset);  // ← Delegated!
    this.indicators.push(...newIndicators);

    // Scoring logic (separate concern)
    return this.evaluateUpgrade();
  }

  // evaluateUpgrade() unchanged - only cares about indicators, not how they're detected
}
```

---

## Usage Examples

### Default (Regex)
```typescript
const detector = ComplexityDetectorFactory.createDefault();  // regex
const scorer = new ProgressiveScorer(detector);
```

### A/B Testing
```typescript
// Try different approaches side by side
const regexDetector = new RegexComplexityDetector();
const embeddingDetector = new EmbeddingComplexityDetector();

const regexScorer = new ProgressiveScorer(regexDetector);
const embeddingScorer = new ProgressiveScorer(embeddingDetector);

// Compare results
```

### Easy Removal
```typescript
// If regex doesn't work in Phase 3, swap it out:
const newDetector = new MLComplexityDetector();  // Drop in replacement!
const scorer = new ProgressiveScorer(newDetector);
```

---

## Architecture Comparison (ORM Pattern)

**This follows the SAME pattern as our ORM:**

| Layer | ORM Example | Complexity Detection |
|-------|-------------|---------------------|
| **Interface** | `DataAdapter` | `ComplexityDetector` |
| **Implementation 1** | `SQLiteAdapter` | `RegexComplexityDetector` |
| **Implementation 2** | `JSONAdapter` | `EmbeddingComplexityDetector` |
| **Implementation 3** | `PostgresAdapter` | `MLComplexityDetector` |
| **Consumer** | `DataDaemon` | `ProgressiveScorer` |
| **Factory** | DataAdapterFactory | ComplexityDetectorFactory |

**Key Benefits:**
- ✅ Plug and play: Swap implementations without touching consumer
- ✅ A/B testable: Run multiple approaches simultaneously
- ✅ Easy removal: If one approach fails, drop it in and use another
- ✅ Clear separation: "What to detect" vs "How to detect"
- ✅ Future-proof: Add new detectors without redesigning system

---

## Implementation Status

**✅ COMPLETED:**
- ComplexityDetector.ts (interface + factory)
- RegexComplexityDetector.ts (extracted patterns from ProgressiveScorer)

**🚧 IN PROGRESS:**
- Refactor ProgressiveScorer to accept detector via constructor
- Remove hard-coded detectIndicators() method
- Update to use this.detector.analyze()

**📋 TODO:**
- Update tests to use factory pattern
- Add examples of swapping detectors
- Document how to add new detector implementations

---

## Why This Matters

**User's feedback was right:** The original code violated the plug-and-play architecture that the rest of the system follows.

**Before this refactor:**
- Changing detection approach = rewrite ProgressiveScorer
- Testing alternatives = complex branching logic
- Removing regex = major surgery

**After this refactor:**
- Changing detection approach = swap 1 line of code
- Testing alternatives = instantiate multiple detectors
- Removing regex = plug in different implementation

This is the difference between **flat code** (mixed concerns) and **layered architecture** (pure abstractions + partial implementations).
