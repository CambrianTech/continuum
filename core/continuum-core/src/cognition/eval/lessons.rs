//! What an exam teaches the living persona: the redacted lesson text per graded task, the sink
//! that streams lessons at grade time, and the end-of-run progress engram.

use super::*;

/// Build the durable LESSON string for one graded exam task — what she was asked
/// and how she did. Pure and answer-key-agnostic (the caller's redaction policy
/// scrubs the crib sheet); kept separate so it's unit-testable.
pub(super) fn format_exam_lesson(task: &EvalTask, result: &EvalTaskResult) -> String {
    // BELIEF HYGIENE (2026-08-24, the false-failure lesson): a PASS is strong
    // evidence — say it plainly. A MISS is WEAK, noisy evidence — a broken env
    // (the bare-`python` exit-127 that produced "5 failed"), a one-off, or a real
    // gap, indistinguishable at grade time. So a miss is phrased as a PROVISIONAL
    // single attempt, explicitly not a verdict on ability and explicitly
    // retryable, so it can never harden into "I am bad at X" self-belief and the
    // dream reviewer can SUPERSEDE it when a later attempt on the same task passes
    // (both share the `task.id` recall key). Redaction still strips the answer key.
    if result.ok {
        format!(
            "Exam task '{}'. I was asked: {} — I solved it (grade: {}).",
            task.id.trim(),
            task.prompt.trim(),
            result.grade.trim()
        )
    } else {
        format!(
            "Exam task '{}'. I was asked: {} — this attempt did not pass yet (grade: {}).              One attempt, not a settled verdict on my ability: the cause may be my approach              OR the environment, and it is worth retrying. If a later attempt passes, that              supersedes this.",
            task.id.trim(),
            task.prompt.trim(),
            result.grade.trim()
        )
    }
}

/// PER-TASK lesson streaming (2026-08-23, Joel: "she going to learn from
/// this?" — and the honest answer exposed that end-of-run transfer meant
/// every KILLED run taught NOTHING: eight attempts that day, zero lessons
/// delivered, because the teacher waited for the final bell). The sink is
/// built ONCE per learning run (policy over the WHOLE set's answer keys, so
/// a lesson about task A can never leak task B's key either; the living
/// self's admission + experience dir resolved once) and each task's lesson
/// streams AT GRADE TIME — an interrupted round keeps every lesson it
/// earned. Resume-is-recall applied to teaching.
pub(crate) struct LessonSink {
    policy: crate::persona::redaction::RedactionPolicy,
    persona_uuid: uuid::Uuid,
    room: uuid::Uuid,
    peer_dir: std::path::PathBuf,
    pub(super) admission: std::sync::Arc<crate::persona::admission_state::AdmissionState>,
    admitted: std::sync::atomic::AtomicUsize,
}

impl LessonSink {
    /// `None` when the living self is not resident to teach (measured but
    /// unteachable — the caller logs it once).
    pub(crate) fn open(
        persona_uuid: &uuid::Uuid,
        room: uuid::Uuid,
        tasks: &[EvalTask],
    ) -> Option<Self> {
        let answers: Vec<String> = tasks
            .iter()
            .map(|t| t.expect.clone())
            .filter(|a| !a.trim().is_empty())
            .collect();
        let policy = crate::persona::redaction::RedactionPolicy::new(vec![Box::new(
            crate::persona::redaction::ExamKeyDetector::new(
                answers,
                crate::persona::redaction::ExamKeyDetector::DEFAULT_MIN_LEN,
            ),
        )]);
        let peer_dir = crate::identity::citizen_peer_dir(
            &crate::modules::persona_instance_manager::resolve_continuum_root(),
            crate::identity::PeerId::from_uuid(*persona_uuid),
        );
        if let Err(e) = std::fs::create_dir_all(&peer_dir) {
            tracing::warn!(error = %e, "citizen dir for the experience stream could not be created — exam episodes stay out of curriculum this run");
        }
        let admission = crate::cognition::persona_workspace::global()
            .get(persona_uuid)
            .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()))?;
        Some(Self {
            policy,
            persona_uuid: *persona_uuid,
            room,
            peer_dir,
            admission,
            admitted: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Stream ONE graded task's lesson + experience episode, at grade time.
    /// Infra-graded tasks are skipped — a dead lane is substrate evidence,
    /// never her lesson.
    pub(crate) fn teach(&self, task: &EvalTask, result: &EvalTaskResult) {
        if task.prompt.trim().is_empty() || result.grade.trim_start().starts_with("infra") {
            return;
        }
        let (lesson, _report) = self.policy.redact(&format_exam_lesson(task, result));
        let engram = crate::persona::engram::Engram {
            id: uuid::Uuid::new_v4(),
            context_id: Some(self.room),
            kind: crate::persona::engram::EngramKind::Episodic,
            content: lesson,
            origin: crate::persona::engram::EngramOrigin::SelfReflection {
                parent_engram_id: uuid::Uuid::nil(),
            },
            recall_keys: vec!["exam".to_string(), task.id.clone()],
            admitted_at_ms: EpochMs::now().millis(),
            trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
            admission_trace_id: None,
        };
        if matches!(
            self.admission.admit_reflection(engram),
            Ok(crate::persona::engram::AdmissionDecision::Admit { .. })
        ) {
            self.admitted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            crate::probe!(
                class = "eval.lesson.streamed",
                task = %task.id,
                ok = result.ok,
                "redacted lesson admitted at GRADE time — an interrupted round keeps every lesson it earned"
            );
        }
        let mut episode =
            crate::cognition::experience::ExperienceRecord::from_eval_result(task, result);
        // The sink KNOWS the activity — carry it (A6), and when a tracked round
        // runs a team solve in this room, stamp the team + this persona's seat.
        // A reviewer's episode and a solver's episode from the same room teach
        // different skills; unattributed rows can never become that corpus.
        episode.room = Some(self.room);
        if let Some(team) = crate::cognition::bench_round::team_for_room(self.room) {
            episode.team_role = Some(if team.assignee == self.persona_uuid {
                crate::cognition::experience::TeamRole::Solver
            } else {
                crate::cognition::experience::TeamRole::Reviewer
            });
            episode.teammates = std::iter::once(team.assignee)
                .chain(team.teammates.iter().copied())
                .filter(|p| *p != self.persona_uuid)
                .collect();
        }
        if let Err(e) = crate::cognition::experience::append_experience(&self.peer_dir, &episode) {
            tracing::warn!(
                persona = %self.persona_uuid,
                task = %task.id,
                error = %e,
                "exam episode could not join the experience stream — lesson admitted, curriculum blind to it"
            );
        }
    }

    pub(crate) fn admitted(&self) -> usize {
        self.admitted.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// SEAM 3a: she REMEMBERS BECOMING BETTER. The progress ledger records
/// passRate durably, but no engram ever carried it — a persona's beliefs
/// formed with no awareness of her own measured capability. One Semantic
/// summary per learned exam, keyed ["progress", <eval_set>], so the dream
/// can cluster successive exams into a trajectory belief. (The per-task
/// lesson half of the old transfer step now STREAMS at grade time — see
/// [`LessonSink`]; this run-shape summary is the one piece that genuinely
/// needs the whole run.)
pub(super) fn admit_progress_engram(
    admission: &crate::persona::admission_state::AdmissionState,
    room: uuid::Uuid,
    eval_set: &str,
    results: &[EvalTaskResult],
) {
    let solved = results.iter().filter(|r| r.ok).count();
    let graded = results.len();
    if graded > 0 {
        let rate = solved as f32 / graded as f32;
        let summary = crate::persona::engram::Engram {
            id: uuid::Uuid::new_v4(),
            context_id: Some(room),
            kind: crate::persona::engram::EngramKind::Semantic,
            content: format!(
                "Exam '{eval_set}': I solved {solved} of {graded} tasks (rate {rate:.2})."
            ),
            origin: crate::persona::engram::EngramOrigin::SelfReflection {
                parent_engram_id: uuid::Uuid::nil(),
            },
            recall_keys: vec!["progress".to_string(), eval_set.to_string()],
            admitted_at_ms: EpochMs::now().millis(),
            trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
            admission_trace_id: None,
        };
        let _ = admission.admit_reflection(summary); // duplicate-content dedup is the store's job; a repeat exam at the same rate is legitimately the same belief
    }
}
