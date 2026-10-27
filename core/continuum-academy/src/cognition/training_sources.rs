//! The training buckets and example sources cognition judges by: the speech-discipline
//! trait (its gym lives in `cognition::gym`) and the `source` values an example carries
//! when no card produced it (read by `cognition::held_out`). Persona's training producer
//! writes them; one owner, below both.

/// THE GATE BECOMES A LESSON (card 657e74de; Joel: "ideally they learn too"). Every
/// echo shape gated at the Speak seam (`framing_echo`, `not_speech`) returns a Pass
/// the citizen never sees — the substrate silences her and she learns nothing, so
/// the same tier produces the same echo next turn. The corpus is `{prompt,
/// completion}` SFT only (no chosen/rejected schema), so a gate verdict cannot be a
/// NEGATIVE; but for the echo class the correct completion IS known and fits the
/// schema: the silence token. This is the one place a gate verdict turns into an
/// example.
///
/// Rules that keep this from teaching muteness, each load-bearing:
/// - only an UNDIRECTED, LIVED turn (the caller checks): a directed question must
///   never train `PASS`, and a synthetic burst is not experience;
/// - its own bucket, [`SPEECH_DISCIPLINE_TRAIT`], never the conversation domain, so
///   no chat gene's set is diluted with silence; and the bucket's gene is adoptable
///   only through its gym (`docs/genome/speech-discipline.jsonl`), which measures
///   BOTH halves — a quiet burst that must end in silence and a real ask that must
///   be answered — so a gene that learned to pass everything fails the gym and is
///   never paged in ([[fallbacks-are-illegal-fail-loud]]);
/// - `quality = 1.0`: a curated domain with its own floor — a 4-char completion
///   scores below `MIN_TRAINING_QUALITY` by construction, and the chat score was
///   built for a different shape.
pub const SPEECH_DISCIPLINE_TRAIT: &str = "speech-discipline";

/// The `source` an example carries when no card produced it: a received lesson or a gated
/// echo. ONE list, written by persona's training producers and read by the held-out judge
/// (`cognition::held_out::trained`), where it is the explicit record that such an example
/// worked no graded task (goal 8). It is the value already on the wire, so a replayed
/// submission keeps its exact bytes.
pub const RECEIVED_LESSON_SOURCE: &str = "received-lesson";

pub const SPEECH_DISCIPLINE_SOURCE: &str = "speech-discipline";

pub const UNLINKED_SOURCES: [&str; 2] = [RECEIVED_LESSON_SOURCE, SPEECH_DISCIPLINE_SOURCE];
