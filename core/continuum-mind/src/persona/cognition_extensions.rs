//! Persona fills cognition's extension slots (`cognition::extensions`) at boot: the verbs
//! cognition calls that persona owns. One list, so the core's boot and a test that drives a
//! real turn install the same set. Each install replaces a previous one; installing twice is
//! harmless.

use std::sync::Arc;

use uuid::Uuid;

use crate::cognition::extensions::{self as ext, CreditSink, FleetLease, SlotFuture};
use crate::persona::PersonaAircRuntimeRegistry;

/// Install every slot.
pub fn install() {
    ext::install_room_voice(room_voice);
    ext::install_card_carry(crate::persona::card_carry::request);
    ext::install_credit_sink(Arc::new(TrainingCredit));
    ext::install_cognition_pulse(crate::persona::cognition_pulse::touch);
    ext::install_mark_perceived(crate::persona::perception_feed::mark_perceived);
    ext::install_remote_binding(|root, name| {
        crate::persona::remote_lane_factory::is_remote_bound(root, name).map_err(|e| e.to_string())
    });
    ext::install_quiesce_fleet(quiesce_fleet);
    ext::install_card_room(card_room);
    ext::install_pinned_workspace_map(|persona, root| {
        Arc::new(crate::persona::workspace_map_source::WorkspaceMapSource::for_pinned_root(persona, root.to_path_buf()))
    });
}

/// Her room voice is her airc citizen; no runtime on this node means nothing to speak through.
fn room_voice(persona: Uuid, room: Uuid, text: String) -> SlotFuture<Option<Result<(), String>>> {
    Box::pin(async move {
        let runtime = PersonaAircRuntimeRegistry::try_global()?.get(persona)?;
        Some(
            crate::persona::airc_citizen::publish_text_in_room(runtime.airc(), room, &text)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
        )
    })
}

/// The room whose board holds `card`, by the one definition (`work_board::horizon`).
fn card_room(persona: Uuid, card: Uuid) -> SlotFuture<Option<Uuid>> {
    Box::pin(async move {
        let runtime = PersonaAircRuntimeRegistry::try_global()?.get(persona)?;
        crate::persona::work_board::horizon::room_holding_card(runtime.airc(), airc_lib::WorkCardId::from_uuid(card))
            .await
            .map(|room| room.channel.as_uuid())
    })
}

fn quiesce_fleet() -> Option<FleetLease> {
    let lease = PersonaAircRuntimeRegistry::try_global()?.quiesce_all();
    Some(FleetLease::new(lease.count(), Box::new(lease)))
}

/// Cognition's credit judgments land in persona's training producer.
struct TrainingCredit;

impl CreditSink for TrainingCredit {
    fn speech_discipline(&self, persona_id: Uuid, persona_name: String, base_model: String, burst: String, marker: &'static str) {
        crate::persona::training_producer::produce_speech_discipline(persona_id, persona_name, base_model, burst, marker);
    }

    fn settle_instance(&self, instance: &str, passed: bool) {
        crate::persona::training_producer::settle_instance_credit(instance, passed);
    }

    fn settle_card(&self, card: Uuid, passed: bool) -> SlotFuture<()> {
        Box::pin(crate::persona::training_producer::settle_card_credit(card, passed))
    }
}
