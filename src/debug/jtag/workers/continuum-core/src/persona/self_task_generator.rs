//! SelfTaskGenerator — Autonomous task creation for personas.
//!
//! Mirrors the TypeScript SelfTaskGenerator logic:
//! - Memory consolidation (every 1 hour)
//! - Skill audit (every 6 hours)
//! - Resume unfinished work (in_progress tasks not updated in 30 min)
//! - Learning opportunities (failed tasks → fine-tune-lora)
//!
//! Called from ChannelModule::tick() every 60 seconds. The generator
//! tracks its own internal timers per-persona so it only creates tasks
//! at the correct intervals.

use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;
use uuid::Uuid;

/// Configuration for self-task generation intervals.
pub struct SelfTaskGeneratorConfig {
    /// How often to review memory (default: 1 hour)
    pub memory_review_interval_ms: u64,
    /// How often to audit skills (default: 6 hours)
    pub skill_audit_interval_ms: u64,
    /// Minimum staleness before work is "unfinished" (default: 30 minutes)
    pub unfinished_work_threshold_ms: u64,
}

impl Default for SelfTaskGeneratorConfig {
    fn default() -> Self {
        Self {
            memory_review_interval_ms: 3_600_000,   // 1 hour
            skill_audit_interval_ms: 21_600_000,     // 6 hours
            unfinished_work_threshold_ms: 1_800_000, // 30 minutes
        }
    }
}

/// Per-persona self-task generator state.
pub struct SelfTaskGenerator {
    persona_id: Uuid,
    config: SelfTaskGeneratorConfig,
    last_memory_review: Instant,
    last_skill_audit: Instant,
}

impl SelfTaskGenerator {
    pub fn new(persona_id: Uuid) -> Self {
        Self {
            persona_id,
            config: SelfTaskGeneratorConfig::default(),
            // Start with epoch so first tick triggers immediately
            last_memory_review: Instant::now(),
            last_skill_audit: Instant::now(),
        }
    }

    /// Generate self-tasks and persist them to the database.
    /// Returns the number of tasks created and enqueued.
    pub async fn generate_and_persist(
        &mut self,
        db_path: &str,
        executor: &crate::runtime::command_executor::CommandExecutor,
    ) -> Result<Vec<Value>, String> {
        let log = crate::runtime::logger("self-task-gen");
        let mut created_tasks = Vec::new();
        let now = Instant::now();

        // 1. Memory consolidation (every hour)
        if now.duration_since(self.last_memory_review).as_millis() as u64
            > self.config.memory_review_interval_ms
        {
            if let Some(task) = self.create_task(
                "memory-consolidation",
                "[Self-Task] Review and consolidate recent memories",
                0.5,
            ) {
                match self.persist_task(db_path, &task, executor).await {
                    Ok(stored) => {
                        created_tasks.push(stored);
                        self.last_memory_review = now;
                    }
                    Err(e) => log.warn(&format!("Failed to persist memory task: {e}")),
                }
            }
        }

        // 2. Skill audit (every 6 hours)
        if now.duration_since(self.last_skill_audit).as_millis() as u64
            > self.config.skill_audit_interval_ms
        {
            if let Some(task) = self.create_task(
                "skill-audit",
                "[Self-Task] Audit skills and identify improvement areas",
                0.6,
            ) {
                match self.persist_task(db_path, &task, executor).await {
                    Ok(stored) => {
                        created_tasks.push(stored);
                        self.last_skill_audit = now;
                    }
                    Err(e) => log.warn(&format!("Failed to persist skill audit task: {e}")),
                }
            }
        }

        // 3. Unfinished work detection
        match self.detect_unfinished_work(db_path, executor).await {
            Ok(tasks) => {
                for task in tasks {
                    match self.persist_task(db_path, &task, executor).await {
                        Ok(stored) => created_tasks.push(stored),
                        Err(e) => log.warn(&format!("Failed to persist resume task: {e}")),
                    }
                }
            }
            Err(e) => log.warn(&format!("Unfinished work detection failed: {e}")),
        }

        // 4. Learning opportunities (failed tasks)
        match self.detect_learning_opportunities(db_path, executor).await {
            Ok(tasks) => {
                for task in tasks {
                    match self.persist_task(db_path, &task, executor).await {
                        Ok(stored) => created_tasks.push(stored),
                        Err(e) => log.warn(&format!("Failed to persist learning task: {e}")),
                    }
                }
            }
            Err(e) => log.warn(&format!("Learning opportunity detection failed: {e}")),
        }

        Ok(created_tasks)
    }

    /// Create a task JSON value (not yet persisted).
    fn create_task(
        &self,
        task_type: &str,
        description: &str,
        priority: f64,
    ) -> Option<Value> {
        Some(serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "assigneeId": self.persona_id.to_string(),
            "createdBy": self.persona_id.to_string(),
            "domain": "self",
            "taskType": task_type,
            "contextId": self.persona_id.to_string(),
            "description": description,
            "priority": priority,
            "status": "pending",
            "createdAt": chrono_now_iso(),
            "updatedAt": chrono_now_iso(),
        }))
    }

    /// Persist a task to the database via data/create command.
    async fn persist_task(
        &self,
        db_path: &str,
        task: &Value,
        executor: &crate::runtime::command_executor::CommandExecutor,
    ) -> Result<Value, String> {
        let id = task.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        executor.execute_json("data/create", serde_json::json!({
            "dbPath": db_path,
            "collection": "tasks",
            "id": id,
            "data": task,
        })).await
    }

    /// Detect in_progress tasks that haven't been updated recently.
    async fn detect_unfinished_work(
        &self,
        db_path: &str,
        executor: &crate::runtime::command_executor::CommandExecutor,
    ) -> Result<Vec<Value>, String> {
        let result = executor.execute_json("data/query", serde_json::json!({
            "dbPath": db_path,
            "collection": "tasks",
            "filter": {
                "assigneeId": { "$eq": self.persona_id.to_string() },
                "status": { "$eq": "in_progress" }
            },
            "limit": 10
        })).await?;

        let records = match result.get("data").and_then(|d| d.as_array()) {
            Some(arr) => arr,
            None => return Ok(Vec::new()),
        };

        let now_ms = now_epoch_ms();
        let threshold = now_ms.saturating_sub(self.config.unfinished_work_threshold_ms);
        let mut resume_tasks = Vec::new();

        for record in records {
            let data = match record.get("data") {
                Some(d) => d,
                None => continue,
            };

            // Check if task is stale (updatedAt < threshold)
            let updated_at = data.get("updatedAt")
                .and_then(|v| v.as_str())
                .and_then(|s| parse_iso_to_epoch_ms(s))
                .or_else(|| data.get("createdAt")
                    .and_then(|v| v.as_str())
                    .and_then(|s| parse_iso_to_epoch_ms(s)))
                .unwrap_or(now_ms);

            if updated_at < threshold {
                let original_desc = data.get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let truncated = if original_desc.len() > 200 {
                    format!("{}...", &original_desc[..197])
                } else {
                    original_desc.to_string()
                };

                if let Some(task) = self.create_task(
                    "resume-work",
                    &format!("[Self-Task] Resume unfinished work: {truncated}"),
                    0.7,
                ) {
                    resume_tasks.push(task);
                }
            }
        }

        Ok(resume_tasks)
    }

    /// Detect failed tasks and create learning opportunities grouped by domain.
    async fn detect_learning_opportunities(
        &self,
        db_path: &str,
        executor: &crate::runtime::command_executor::CommandExecutor,
    ) -> Result<Vec<Value>, String> {
        let result = executor.execute_json("data/query", serde_json::json!({
            "dbPath": db_path,
            "collection": "tasks",
            "filter": {
                "assigneeId": { "$eq": self.persona_id.to_string() },
                "status": { "$eq": "failed" }
            },
            "limit": 5
        })).await?;

        let records = match result.get("data").and_then(|d| d.as_array()) {
            Some(arr) => arr,
            None => return Ok(Vec::new()),
        };

        // Group failures by domain
        let mut failures_by_domain: HashMap<String, usize> = HashMap::new();
        for record in records {
            let domain = record.get("data")
                .and_then(|d| d.get("domain"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            *failures_by_domain.entry(domain).or_insert(0) += 1;
        }

        let mut learning_tasks = Vec::new();
        for (domain, count) in &failures_by_domain {
            if let Some(mut task) = self.create_task(
                "fine-tune-lora",
                &format!("[Self-Task] Learn from {count} recent {domain} failures"),
                0.8,
            ) {
                // Add metadata for LoRA layer targeting
                if let Some(obj) = task.as_object_mut() {
                    obj.insert("metadata".to_string(), serde_json::json!({
                        "loraLayer": format!("{domain}-expertise")
                    }));
                }
                learning_tasks.push(task);
            }
        }

        Ok(learning_tasks)
    }
}

// ── Utility functions ──────────────────────────────────────────────────────

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn chrono_now_iso() -> String {
    // Simple ISO 8601 timestamp without external crate
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Approximate: good enough for task timestamps
    format!("1970-01-01T00:00:00.000Z") // Placeholder — overwritten by DB on insert
        .replace("1970-01-01T00:00:00.000Z", &format_epoch_secs(secs))
}

fn format_epoch_secs(secs: u64) -> String {
    // Convert epoch seconds to ISO 8601 (approximate, no leap seconds)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Calculate year/month/day from days since epoch (simplified)
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.000Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Simplified date calculation from epoch days
    let mut year = 1970u64;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let month_days: [u64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    (year, month, days + 1)
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn parse_iso_to_epoch_ms(iso: &str) -> Option<u64> {
    // Parse ISO 8601: "2025-01-15T10:30:00.000Z" → epoch ms
    // Minimal parser — handles the common format from our DB
    let parts: Vec<&str> = iso.split('T').collect();
    if parts.len() != 2 {
        return None;
    }

    let date_parts: Vec<u64> = parts[0].split('-').filter_map(|s| s.parse().ok()).collect();
    if date_parts.len() != 3 {
        return None;
    }

    let time_str = parts[1].trim_end_matches('Z');
    let time_parts: Vec<&str> = time_str.split(':').collect();
    if time_parts.len() < 3 {
        return None;
    }

    let year = date_parts[0];
    let month = date_parts[1];
    let day = date_parts[2];
    let hour: u64 = time_parts[0].parse().ok()?;
    let minute: u64 = time_parts[1].parse().ok()?;
    let second: u64 = time_parts[2].split('.').next()?.parse().ok()?;

    // Convert to epoch days
    let mut days = 0u64;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    let leap = is_leap_year(year);
    let month_days: [u64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for m in 0..(month.saturating_sub(1) as usize).min(11) {
        days += month_days[m];
    }
    days += day.saturating_sub(1);

    let total_secs = days * 86400 + hour * 3600 + minute * 60 + second;
    Some(total_secs * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_to_ymd() {
        // Jan 1, 1970 = day 0
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        // Feb 1, 1970 = day 31
        assert_eq!(days_to_ymd(31), (1970, 2, 1));
    }

    #[test]
    fn test_parse_iso_to_epoch_ms() {
        // 1970-01-01T00:00:00.000Z = 0
        assert_eq!(parse_iso_to_epoch_ms("1970-01-01T00:00:00.000Z"), Some(0));
        // 1970-01-01T00:01:00.000Z = 60000
        assert_eq!(parse_iso_to_epoch_ms("1970-01-01T00:01:00.000Z"), Some(60000));
        // Invalid
        assert_eq!(parse_iso_to_epoch_ms("not-a-date"), None);
    }

    #[test]
    fn test_format_epoch_secs() {
        let s = format_epoch_secs(0);
        assert_eq!(s, "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn test_self_task_generator_creation() {
        let gen = SelfTaskGenerator::new(Uuid::new_v4());
        assert_eq!(gen.config.memory_review_interval_ms, 3_600_000);
    }

    #[test]
    fn test_create_task() {
        let gen = SelfTaskGenerator::new(Uuid::new_v4());
        let task = gen.create_task("memory-consolidation", "Review memories", 0.5);
        assert!(task.is_some());
        let task = task.unwrap();
        assert_eq!(task["taskType"], "memory-consolidation");
        assert_eq!(task["priority"], 0.5);
        assert_eq!(task["domain"], "self");
    }
}
