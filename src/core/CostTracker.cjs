/**
 * Cost Tracker
 * Handles cost calculations, persistence, and reporting
 */

const fs = require('fs');
const path = require('path');

class CostTracker {
  constructor(dataFilePath) {
    this.dataFilePath = dataFilePath;
    this.data = this.loadData();
  }

  loadData() {
    try {
      if (fs.existsSync(this.dataFilePath)) {
        const data = JSON.parse(fs.readFileSync(this.dataFilePath, 'utf-8'));
        return {
          total: data.total || 0,
          requests: data.requests || 0,
          byModel: data.byModel || {},
          sessions: data.sessions || {}
        };
      }
    } catch (error) {
      console.log('⚠️  Could not load cost data, starting fresh');
    }
    return {
      total: 0,
      requests: 0,
      byModel: {},
      sessions: {}
    };
  }

  saveData() {
    try {
      const dir = path.dirname(this.dataFilePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(this.dataFilePath, JSON.stringify(this.data, null, 2));
    } catch (error) {
      console.error('❌ Failed to save cost data:', error.message);
    }
  }

  trackCost(modelName, inputTokens, outputTokens, cost) {
    this.data.total += cost;
    this.data.requests++;
    
    if (!this.data.byModel[modelName]) {
      this.data.byModel[modelName] = {
        requests: 0,
        totalCost: 0,
        inputTokens: 0,
        outputTokens: 0
      };
    }
    
    this.data.byModel[modelName].requests++;
    this.data.byModel[modelName].totalCost += cost;
    this.data.byModel[modelName].inputTokens += inputTokens;
    this.data.byModel[modelName].outputTokens += outputTokens;
    
    this.saveData();
  }

  trackSession(sessionId, cost) {
    if (!this.data.sessions[sessionId]) {
      this.data.sessions[sessionId] = { cost: 0, requests: 0 };
    }
    this.data.sessions[sessionId].cost += cost;
    this.data.sessions[sessionId].requests++;
  }

  getTotal() {
    return this.data.total;
  }

  getRequests() {
    return this.data.requests;
  }

  getCostsByModel() {
    return this.data.byModel;
  }

  getFormattedSummary() {
    const total = this.data.total;
    const requests = this.data.requests;
    
    if (total === 0) {
      return "📊 No costs tracked yet";
    }
    
    const avgCost = total / requests;
    return `📊 ${requests} requests | 💰 $${total.toFixed(6)} | ⚡ $${avgCost.toFixed(6)}/req`;
  }

  getDetailedReport() {
    const report = {
      total: this.data.total,
      requests: this.data.requests,
      averageCost: this.data.requests > 0 ? this.data.total / this.data.requests : 0,
      byModel: this.data.byModel,
      sessionCount: Object.keys(this.data.sessions).length
    };
    
    return report;
  }

  reset() {
    this.data = {
      total: 0,
      requests: 0,
      byModel: {},
      sessions: {}
    };
    this.saveData();
  }
}

module.exports = CostTracker;