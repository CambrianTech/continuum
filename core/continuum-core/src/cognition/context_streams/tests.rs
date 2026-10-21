use std::sync::Arc;

use continuum_context::source::Source;

use super::actions::Actions;
use crate::cognition::working_memory::WorkingMemory;

fn rendered(memory: &WorkingMemory) -> String {
    Actions { memory }.items().iter().map(|item| item.full.text.clone()).collect::<Vec<_>>().join("\n")
}

// what this catches: her action stream going silent or lying about her acts, the three
// honest states the steps ledger held (#151, #414). Zero acts: the explicit
// nothing-has-executed void (an implicit one became a confabulation shelter). An act on
// record: listed, the void gone, and still listed after chatty entries flood the
// working-memory ring (the starvation that showed a 2,863-act citizen one act). Acts whose
// details are gone (a counter restored without records): said so, never denied, and no
// promise that recall can retrieve them.
#[test]
fn her_actions_say_honestly_what_has_executed() {
    let memory = Arc::new(WorkingMemory::new(2));
    assert!(rendered(&memory).contains("nothing has executed yet"));

    memory.record_receipt("I ran code/shell(ls) Result: ok");
    memory.record_settlement("listed", None);
    memory.record_fact("chose silence — said nothing to the room");
    memory.record_fact("chose silence — said nothing to the room (again)");
    assert!(!memory.has_receipt(), "precondition: the receipt's ring entry aged out");
    let listed = rendered(&memory);
    assert!(listed.contains("[result #1") && listed.contains("code/shell(ls)"), "{listed}");
    assert!(!listed.contains("nothing has executed yet"), "{listed}");

    let earlier = WorkingMemory::new(2);
    let mut snapshot = WorkingMemory::new(2).snapshot();
    snapshot.next_action_seq = 4; // 3 acts happened in the old life, none recorded
    earlier.restore(snapshot);
    let aged = rendered(&earlier);
    assert!(aged.contains("3 steps executed earlier"), "{aged}");
    assert!(!aged.contains("nothing has executed yet"), "denied her real acts: {aged}");
    assert!(!aged.to_lowercase().contains("recall can retrieve"), "{aged}");
}
