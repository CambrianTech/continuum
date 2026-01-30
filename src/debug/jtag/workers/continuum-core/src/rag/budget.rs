//! RAG Budget Manager
//!
//! Allocates token budget across sources based on priority

use super::types::BudgetAllocation;

/// Source registration for budget allocation
#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub name: String,
    pub priority: u8,           // Higher = more important (0-100)
    pub default_percent: u8,    // Default budget percentage
    pub min_tokens: usize,      // Minimum tokens needed to be useful
}

/// Budget manager allocates tokens across RAG sources
pub struct BudgetManager {
    total_budget: usize,
}

impl BudgetManager {
    pub fn new(total_budget: usize) -> Self {
        Self { total_budget }
    }

    /// Allocate budget across sources based on priority
    pub fn allocate(&self, sources: &[SourceConfig]) -> Vec<BudgetAllocation> {
        if sources.is_empty() {
            return vec![];
        }

        // Calculate total priority weight
        let total_priority: u32 = sources.iter().map(|s| s.priority as u32).sum();
        if total_priority == 0 {
            return vec![];
        }

        // Allocate proportionally by priority
        sources
            .iter()
            .map(|source| {
                let proportion = source.priority as f64 / total_priority as f64;
                let allocated = (self.total_budget as f64 * proportion) as usize;
                let allocated = allocated.max(source.min_tokens);

                BudgetAllocation {
                    source_name: source.name.clone(),
                    allocated_tokens: allocated,
                    priority: source.priority,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_allocation_by_priority() {
        let manager = BudgetManager::new(1000);

        let sources = vec![
            SourceConfig {
                name: "identity".to_string(),
                priority: 95,
                default_percent: 10,
                min_tokens: 100,
            },
            SourceConfig {
                name: "conversation".to_string(),
                priority: 80,
                default_percent: 50,
                min_tokens: 200,
            },
            SourceConfig {
                name: "memory".to_string(),
                priority: 70,
                default_percent: 20,
                min_tokens: 100,
            },
        ];

        let allocations = manager.allocate(&sources);

        assert_eq!(allocations.len(), 3);

        // Identity has highest priority, should get most tokens proportionally
        let identity = allocations.iter().find(|a| a.source_name == "identity").unwrap();
        let conversation = allocations.iter().find(|a| a.source_name == "conversation").unwrap();

        // 95/(95+80+70) = 95/245 ≈ 0.388 → ~388 tokens
        assert!(identity.allocated_tokens > 350);
        assert!(identity.allocated_tokens < 450);

        // Conversation: 80/245 ≈ 0.327 → ~327 tokens
        assert!(conversation.allocated_tokens > 280);
        assert!(conversation.allocated_tokens < 380);
    }

    #[test]
    fn test_minimum_tokens_respected() {
        let manager = BudgetManager::new(100); // Small budget

        let sources = vec![
            SourceConfig {
                name: "identity".to_string(),
                priority: 10,
                default_percent: 10,
                min_tokens: 200, // More than total budget!
            },
        ];

        let allocations = manager.allocate(&sources);

        // Should respect minimum even if over budget
        assert_eq!(allocations[0].allocated_tokens, 200);
    }

    #[test]
    fn test_empty_sources() {
        let manager = BudgetManager::new(1000);
        let allocations = manager.allocate(&[]);
        assert!(allocations.is_empty());
    }
}
