//! The in-process engine's [`LearnPort`] (IN-PROCESS-LEARNING.md, L2): what the trainer tells reaches
//! the persona driver's one queue, a lesson rendered with her chat template first, here on the
//! trainer's thread, so the engine thread never renders or tokenizes.

use crate::inference::backends::persona_scheduler::lesson::{self, LessonRefusal};
use crate::inference::backends::persona_scheduler::{LearningJob, LearningLesson, LearningMeasuring};
use crate::inference::learning::{say, speaks, LearnPort, Learning, Said, StepOutcome, Unavailable};

use super::LlamaCppBackend;

impl LlamaCppBackend {
    /// Her template's rendering of one request: the prompt it produces.
    fn rendered_prompt(&self, request: &serde_json::Value) -> Result<String, LessonRefusal> {
        // The templates load once, at her first turn; a node that learns before it served uses the
        // model's own template, never a guessed one.
        let render = self.render_chat_turn(&request.to_string(), || None, llama::ReasoningFormat::Auto).map_err(LessonRefusal::Chat)?;
        let described = render.describe().map_err(LessonRefusal::Chat)?;
        let described: serde_json::Value = serde_json::from_str(&described).map_err(LessonRefusal::Described)?;
        match described["prompt"].as_str() {
            Some(prompt) => Ok(prompt.to_owned()),
            None => Err(LessonRefusal::NoPrompt),
        }
    }
}

impl LearnPort for LlamaCppBackend {
    fn tell(&self, learning: Learning) -> Result<(), Unavailable> {
        let Some((_, Ok(scheduler))) = self.scheduler.get().filter(|_| speaks()) else {
            return Err(Unavailable { returned: learning });
        };
        let job = match learning {
            Learning::Learn(learn) => {
                let rendered = lesson::render(&learn.lesson, |request| self.rendered_prompt(request), |text| self.model.tokenize(text, true, true).map_err(LessonRefusal::Template));
                match rendered {
                    Ok(rendered) => LearningJob::Learn(LearningLesson { seat: learn.seat, step: learn.step, rendered, admission: learn.admission }),
                    Err(refused) => {
                        say(Said::Step { seat: learn.seat, step: learn.step, outcome: StepOutcome::Refused { why: refused.to_string().into() } });
                        return Ok(());
                    }
                }
            }
            Learning::Measure(measure) => {
                // Sized at the lesson's reply rows: the step prefills its context forward-only (no gradient)
                // and trains only her reply, so a seat is her reply against her context, never the whole
                // lesson from scratch (the 5090, 2026-11-26: whole-lesson sizing quoted 244 GB to 48.8 TB); its
                // attention reads the lesson alone, never the served window (the M5 35B: 78 to 105 GB).
                let rows = lesson::render(&measure.sized_for, |request| self.rendered_prompt(request), |text| self.model.tokenize(text, true, true).map_err(LessonRefusal::Template))
                    .map(|rendered| rendered.rows());
                // A render refusal goes to the driver, which alone knows whether this seat holds anything.
                match rows {
                    Ok(Some(rows)) => LearningJob::Measure(LearningMeasuring { seat: measure.seat, mind: measure.mind, shadow: measure.shadow, optimizer: measure.optimizer, rows }),
                    Ok(None) => LearningJob::Unmeasured { seat: measure.seat, why: "the lesson it is sized for renders to no rows a seat can hold".into() },
                    Err(refused) => LearningJob::Unmeasured { seat: measure.seat, why: refused.to_string().into() },
                }
            }
            told => LearningJob::Told(told),
        };
        scheduler.learn(job);
        Ok(())
    }
}

impl LearnPort for crate::inference::llamacpp_adapter::LlamaCppAdapter {
    /// The backend serving now, if one is loaded.
    fn tell(&self, learning: Learning) -> Result<(), Unavailable> {
        match self.loaded_backend() {
            Some(backend) => backend.tell(learning),
            None => Err(Unavailable { returned: learning }),
        }
    }
}
