//! The comparison table.

use std::fmt::Write;

use crate::cost::CostModel;
use crate::evaluate::Score;

/// One table: a row per policy, the train half then the test half.
pub fn table(title: &str, rows: &[(String, [Score; 2])], cost: &CostModel) -> String {
    let mut out = String::new();
    let calls = |score: &Score| score.calls;
    let (train, test) = rows.first().map_or((0, 0), |(_, halves)| (calls(&halves[0]), calls(&halves[1]))); // map_or: no policies prints an empty table
    let epochs = rows.first().map_or((0, 0), |(_, halves)| (halves[0].epochs, halves[1].epochs)); // map_or: as above
    let _ = writeln!(out, "\n## {title}: train {} epochs / {train} calls, test {} epochs / {test} calls", epochs.0, epochs.1);
    let _ = writeln!(
        out,
        "{:<26} | {:>9} {:>8} {:>9} {:>6} {:>8} | {:>9} {:>8} {:>9} {:>6} {:>8} | {:>5} {:>6}",
        "policy", "miss/call", "waste/ep", "head tok", "head n", "cost s/ep", "miss/call", "waste/ep", "head tok", "head n", "cost s/ep", "churn", "moved%"
    );
    for (name, halves) in rows {
        let _ = write!(out, "{name:<26}");
        for half in halves {
            if half.epochs == 0 {
                let _ = write!(out, " | {:>45}", "-");
                continue;
            }
            let _ = write!(
                out,
                " | {:>9.3} {:>8.0} {:>9.0} {:>6.1} {:>8.1}",
                half.misses as f64 / half.calls.max(1) as f64,
                half.per_epoch(half.waste_tokens as f64),
                half.per_epoch(half.head_tokens as f64),
                half.per_epoch(half.head_tools as f64),
                half.per_epoch(cost.seconds(half.cost_tokens(cost))),
            );
        }
        let churn = halves[0].churn + halves[1].churn;
        let moved = (halves[0].turnover_changes + halves[1].turnover_changes) as f64 * 100.0 / (halves[0].epochs + halves[1].epochs).max(1) as f64;
        let _ = writeln!(out, " | {churn:>5} {moved:>6.0}");
    }
    out
}
