#![cfg(test)]
//! From `persona::airc_persona_conversation`: core's `room/join` and `room/leave` commands
//! refresh the caller's living stream without moving her focus. Asserts exactly what it
//! asserted before the move.

use std::sync::Arc;

use crate::persona::airc_persona_conversation::*;
use crate::cognition::conversation::PersonaConversation;

/// what this catches: e0d64552 — the actual membership commands persisted
/// their changes but bypassed the epoch that refreshes a living stream.
/// This uses an isolated real scope, not a mocked membership notification.
#[tokio::test]
async fn membership_commands_refresh_the_callers_runtime_without_moving_focus() {
    use crate::modules::room::{RoomJoin, RoomJoinParams, RoomLeave, RoomLeaveParams};
    use crate::persona::airc_citizen::AircCitizen;
    use crate::persona::identity_provider::PersonaIdentitySource;
    use crate::persona::PersonaAircRuntime;
    use crate::persona::PersonaAircRuntimeRegistry;
    use crate::routing::CallerIdentity;
    use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};
    use airc_core::PeerId;

    let home = tempfile::tempdir().expect("isolated airc home and wire root");
    let airc = Arc::new(
        airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await
            .expect("open without the installed daemon or account wire"),
    );
    let academy = airc.join("academy").await.expect("initial focus");
    let registry = PersonaAircRuntimeRegistry::new();
    let runtime = registry.register(PersonaAircRuntime::from_attached(
        airc.peer_id().as_uuid(),
        "membership-test",
        home.path().to_path_buf(),
        airc.clone(),
        academy.channel,
        PersonaIdentitySource::FreshlyMinted,
    ));
    let ctx = Ctx {
        caller: Some(CallerIdentity::local_persona(airc.peer_id())),
        ..Default::default()
    };
    let join = RoomJoin {
        registry: registry.clone(),
    };
    let leave = RoomLeave { registry };
    // The live conversation takes this receiver before subscribing. A raw
    // Airc mutation below would pass the durable assertions but fail this one.
    let mut epoch = runtime.membership_epoch();
    let mut conversation = AircPersonaConversation::new(runtime.clone());
    conversation
        .prime()
        .await
        .expect("prime before the membership command");
    let joined = join
        .run(
            &ctx,
            RoomJoinParams {
                room: "widgets".into(),
            },
        )
        .await
        .expect("join through the authenticated command");
    assert!(epoch.has_changed().expect("runtime still owns the sender"));
    epoch.borrow_and_update();
    assert!(
        !joined.is_default,
        "joining must not move the existing focus"
    );
    let membership = airc.subscription_set().await.expect("joined subscriptions");
    assert!(membership
        .all()
        .any(|s| s.room_id.as_uuid().to_string() == joined.room_id));
    assert_eq!(
        membership.default_subscription().expect("default").room_id,
        academy.channel
    );

    // The command updates the same durable identity used after a restart.
    let reopened = airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
        .await
        .expect("reopen the same isolated scope");
    assert_eq!(reopened.peer_id(), airc.peer_id());
    assert_eq!(
        reopened
            .subscription_set()
            .await
            .expect("persisted membership"),
        membership
    );
    drop(reopened);

    join.run(
        &ctx,
        RoomJoinParams {
            room: "widgets".into(),
        },
    )
    .await
    .expect("repeated join is membership-idempotent");
    assert_eq!(
        airc.subscription_set().await.expect("same rooms"),
        membership
    );
    // Preserve the existing runtime contract: a successful repeat refreshes
    // the snapshot too, without duplicating membership or changing focus.
    assert!(epoch.has_changed().expect("sender alive"));
    epoch.borrow_and_update();

    for invalid in [" ".to_string(), uuid::Uuid::new_v4().to_string()] {
        assert!(join
            .run(&ctx, RoomJoinParams { room: invalid })
            .await
            .is_err());
        assert!(!epoch.has_changed().expect("sender alive"));
        assert_eq!(
            airc.subscription_set().await.expect("unchanged membership"),
            membership
        );
    }
    let unknown = Ctx {
        caller: Some(CallerIdentity::local_persona(PeerId::new())),
        ..Default::default()
    };
    assert!(matches!(
        join.run(
            &unknown,
            RoomJoinParams {
                room: "unrelated".into()
            }
        )
        .await,
        Err(CommandError::NotFound(_))
    ));
    assert!(matches!(
        leave
            .run(
                &unknown,
                RoomLeaveParams {
                    room: Some("widgets".into())
                }
            )
            .await,
        Err(CommandError::NotFound(_))
    ));
    assert!(!epoch
        .has_changed()
        .expect("an unknown caller cannot mutate this runtime"));
    assert_eq!(
        airc.subscription_set()
            .await
            .expect("same caller membership"),
        membership
    );

    let parted = leave
        .run(
            &ctx,
            RoomLeaveParams {
                room: Some("widgets".into()),
            },
        )
        .await
        .expect("leave through the authenticated command");
    assert_eq!(parted.room_id, joined.room_id);
    assert_eq!(parted.remaining, 1);
    assert!(epoch
        .has_changed()
        .expect("part refreshes the live snapshot"));
    epoch.borrow_and_update();
    let remaining = airc.subscription_set().await.expect("parted subscriptions");
    assert!(!remaining
        .all()
        .any(|s| s.room_id.as_uuid().to_string() == joined.room_id));
    assert_eq!(
        remaining.default_subscription().expect("default").room_id,
        academy.channel
    );
    assert!(leave
        .run(
            &ctx,
            RoomLeaveParams {
                room: Some("widgets".into())
            }
        )
        .await
        .is_err());
    assert!(!epoch
        .has_changed()
        .expect("a rejected repeat must not signal a change"));
    assert_eq!(
        airc.subscription_set()
            .await
            .expect("unchanged after refusal"),
        remaining
    );

    let parted_default = leave
        .run(&ctx, RoomLeaveParams::default())
        .await
        .expect("unnamed leave targets the existing default");
    assert_eq!(
        parted_default.room_id,
        academy.channel.as_uuid().to_string()
    );
    assert_eq!(parted_default.remaining, 0);
    assert!(epoch
        .has_changed()
        .expect("leaving the default also refreshes"));
    let empty = airc.subscription_set().await.expect("no rooms remain");
    assert!(empty.default_subscription().is_none());
    assert_eq!(empty.all().count(), 0);

    // Drive the ACTUAL epoch consumer. Before the fix this reattached with
    // the SDK's empty-set general fallback, then default-room replay durably
    // rejoined general. Asserting the snapshot and released pump proves the
    // epoch was processed, rather than merely timing out before the branch.
    let wait = std::time::Duration::from_secs(1);
    assert!(tokio::time::timeout(wait, conversation.next_message())
        .await
        .is_err());
    assert_eq!(conversation.subscribed_rooms(), Some(&[][..]));
    assert_eq!(conversation.pump_attached(), (false, false));
    assert_eq!(
        airc.subscription_set().await.expect("parked membership"),
        empty
    );
    assert_eq!(
        conversation
            .high_water_mark(32)
            .await
            .expect("empty history"),
        0
    );

    let mut started_empty = AircPersonaConversation::new(runtime.clone());
    started_empty
        .prime()
        .await
        .expect("priming an empty citizen parks");
    assert_eq!(started_empty.subscribed_rooms(), Some(&[][..]));
    assert_eq!(started_empty.pump_attached(), (false, false));
    assert_eq!(
        airc.subscription_set().await.expect("empty after prime"),
        empty
    );

    let rejoined = join
        .run(
            &ctx,
            RoomJoinParams {
                room: "widgets".into(),
            },
        )
        .await
        .expect("a later join resumes the same living conversation");
    assert!(tokio::time::timeout(wait, conversation.next_message())
        .await
        .is_err());
    let resumed = uuid::Uuid::parse_str(&rejoined.room_id).expect("room receipt UUID");
    assert_eq!(conversation.subscribed_rooms(), Some(&[resumed][..]));
    assert_eq!(conversation.pump_attached(), (true, true));
    assert_eq!(
        airc.subscription_set()
            .await
            .expect("resumed membership")
            .all()
            .count(),
        1
    );

    leave
        .run(&ctx, RoomLeaveParams::default())
        .await
        .expect("finish parked");
    assert!(tokio::time::timeout(wait, conversation.next_message())
        .await
        .is_err());
    assert_eq!(conversation.pump_attached(), (false, false));
    assert_eq!(
        airc.subscription_set()
            .await
            .expect("final empty membership")
            .all()
            .count(),
        0
    );
}
