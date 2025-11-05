# 🎲 Stochastic Priority Queue System

**Mathematical Foundation for Fair Message Scheduling**

This directory contains a sophisticated stochastic scheduling algorithm that solves the classic computer science priority queue starvation problem using exponential age-weighted probability distribution.

## 📈 Mathematical Foundation

### **Core Formula**
```
P(age) = baseProb + (ageFactor × (1 - e^(-growthRate × age_seconds)))
```

**Where:**
- `baseProb = 0.1` (10% minimum chance for new messages)
- `ageFactor = 0.9` (90% maximum boost from aging)  
- `growthRate = 0.002` (exponential growth rate per second)

### **Probability Progression**
- **New messages (0s)**: P = 0.100 (10% chance)
- **5 minutes old**: P = 0.506 (50.6% chance)
- **30 minutes old**: P = 0.975 (97.5% chance)
- **1 hour old**: P = 0.999 (99.9% chance - virtually guaranteed)

## 🎯 Key Features

### **1. Fair Percentage Allocation**
- **CRITICAL**: 40% of processing capacity
- **HIGH**: 40% of processing capacity  
- **NORMAL**: 15% of processing capacity
- **LOW**: 5% of processing capacity

### **2. Age-Based Promotion**
- Messages older than 30 seconds get promoted to HIGH priority
- Prevents indefinite queuing of any message
- Creates natural escalation path for urgent items

### **3. Stochastic Selection**
- Uses weighted random selection within priority bands
- Prevents deterministic patterns that could cause subtle starvation
- Provides mathematical fairness guarantees

### **4. Eventual Delivery Guarantee**
- **No message can be starved indefinitely**
- Exponential probability growth ensures ancient messages get processed
- Maintains fairness while respecting priority ordering

## 🧪 Tested & Verified

The system includes comprehensive unit tests (`stochastic-priority-queue.test.ts`) that verify:

- ✅ **Basic queue operations** (enqueue, dequeue, clear)
- ✅ **Priority ordering** with stochastic selection
- ✅ **Age-based promotion** (30s threshold → HIGH priority)
- ✅ **Exponential probability formula** accuracy
- ✅ **Fair percentage allocation** (40% CRITICAL/HIGH, 15% NORMAL, 5% LOW)
- ✅ **Eventual delivery guarantee** (prevents starvation)
- ✅ **Queue size limiting** and priority-based eviction
- ✅ **Retry mechanism** with exponential backoff

### **Test Results Example**
```
📊 Selection distribution: new=3, medium=13, old=34
📊 Processed distribution: CRITICAL=8, HIGH=8, NORMAL=3, LOW=1
🎯 Queue: Stochastic selection - aged item (1800s) with P=0.975
⏰ Queue: Promoting aged message (3600s old) from LOW to HIGH
```

## 🔬 Computer Science Foundation

This implementation draws from classic scheduling algorithms:

### **Similar to Loss Functions**
- **Exponential growth** creates natural "selection pressure"
- **Probabilistic optimization** like gradient descent
- **Convergence guarantees** ensure eventual processing

### **Similar to RANSAC**
- **Stochastic sampling** for robust estimation
- **Probability-driven selection** with mathematical guarantees
- **Outlier handling** (very old messages get priority boost)

### **Classic Kernel Scheduling**
- **Weighted Fair Queuing (WFQ)** percentage allocation
- **Completely Fair Scheduler (CFS)** age-based promotion
- **Multi-Level Feedback Queue (MLFQ)** priority escalation

## 🎮 Usage

```typescript
import { PriorityQueue, Priority } from './PriorityQueue';

const queue = new PriorityQueue<MessageType>({
  maxSize: 1000,
  maxRetries: 3,
  flushInterval: 500  // Process every 500ms
});

// Enqueue with priority
const messageId = queue.enqueue(message, Priority.NORMAL);

// Start stochastic processing
queue.startProcessing(async (items) => {
  // Process batch of fairly-selected items
  const failed = await processBatch(items);
  return failed; // Return failed items for retry
});

// Queue automatically handles:
// - Age-based promotion (30s → HIGH priority)
// - Stochastic fair selection within priority bands  
// - Exponential retry backoff for failed items
// - Mathematical fairness guarantees
```

## 🌟 Why This Matters

Traditional priority queues suffer from **starvation** - low priority items can wait indefinitely if high priority items keep arriving. This system solves that with:

1. **Mathematical Guarantees**: Every message has provable eventual delivery
2. **Fair Resource Allocation**: Percentage-based distribution respects priorities while ensuring fairness
3. **Real-World Performance**: Used in production for console logging, message routing, and cross-context communication
4. **Elegant Simplicity**: Complex fairness achieved through simple exponential formula

## 📚 Files

- **`PriorityQueue.ts`** - Main stochastic priority queue implementation
- **`JTAGMessageQueue.ts`** - JTAG-specific message queue wrapper
- **`../../../tests/unit/stochastic-priority-queue.test.ts`** - Comprehensive unit tests

## 🎲 The Beauty of Stochastic Scheduling

*"Just like loss functions, just like RANSAC - probability-driven selection that grows with need. That's how you do it stochastically. This is well talked about in computing."*

The exponential probability curve creates natural fairness - fresh messages get a chance, aging messages get increasing priority, and ancient messages become virtually guaranteed for processing. Mathematical elegance solving a classic computer science problem.

---

*This system powers the JTAG message routing infrastructure, ensuring fair and reliable message delivery across browser-server boundaries with mathematical precision.*