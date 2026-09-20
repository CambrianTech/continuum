//! ONE place that turns "the room a verb names" into an [`airc_lib::Room`].
//!
//! Joel, 2026-09-05: "You never work in rooms. You just use whatever one of
//! you reuses." Every activity / work verb that means a specific room must
//! address it by id or name; the scope's current-room pointer is the fallback
//! only when nothing was named, never the reason a card or a standing lands
//! somewhere. `activity/archive`, `activity/protect` and `work/create` all
//! resolve through here so the rule has exactly one implementation.

use airc_lib::Airc;

use crate::sdk_codegen::CommandError;

/// Resolve the room a verb names — by id or by name — among the rooms the
/// caller is subscribed to, or the caller's current room when none is named.
///
/// Subscription is the reach: a room the caller is not in has no wall this
/// handle can read or write (the same bound `AircRecipeReader::recipe_posts`
/// states), so an unknown room is refused by name with the rooms that ARE in
/// reach, never silently swapped for the current one.
pub(crate) async fn resolve_room(airc: &Airc, named: Option<&str>) -> Result<airc_lib::Room, CommandError> {
    let Some(named) = named.map(str::trim).filter(|s| !s.is_empty()) else {
        return airc.current_room().await.map_err(|source| {
            CommandError::Internal(format!("could not resolve the current room: {source}"))
        });
    };
    let set = airc.subscription_set().await.map_err(|source| {
        CommandError::Internal(format!("subscription set unavailable: {source}"))
    })?;
    let wanted = named.trim_start_matches('#');
    let by_id = uuid::Uuid::parse_str(wanted).ok();
    let mut names = Vec::new();
    for sub in set.all() {
        let room = sub.as_room();
        if room.name == wanted || by_id == Some(room.channel.as_uuid()) {
            return Ok(room);
        }
        names.push(format!("#{}", room.name));
    }
    Err(CommandError::Invalid(format!(
        "room {named:?} is not among the rooms this caller is in ({}) — join it first          (room/join), or name one of these",
        names.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One local scope subscribed to two rooms — the shape every activity verb
    /// runs in: a pointer on one room, a verb that means another.
    async fn two_room_scope() -> (tempfile::TempDir, Airc) {
        let home = tempfile::tempdir().expect("temp airc home");
        let airc = Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await
            .expect("a local airc scope opens without a daemon");
        airc.join("academy").await.expect("join academy");
        airc.join("widgets").await.expect("join the project room");
        (home, airc)
    }

    // what this catches: the pointer default sneaking back — a verb naming a room
    // by NAME or by ID must get THAT room even while the scope points elsewhere.
    #[tokio::test]
    async fn a_named_room_resolves_by_name_or_id_regardless_of_the_pointer() {
        let (_home, airc) = two_room_scope().await;
        airc.join("academy").await.expect("point the scope at academy");
        let current = airc.current_room().await.expect("current room");
        assert_eq!(current.name, "academy", "the pointer stands on academy");

        let by_name = resolve_room(&airc, Some("#widgets")).await.expect("by name");
        assert_eq!(by_name.name, "widgets");
        let id = by_name.channel.as_uuid().to_string();
        let by_id = resolve_room(&airc, Some(&id)).await.expect("by id");
        assert_eq!(by_id.channel, by_name.channel);

        let none = resolve_room(&airc, None).await.expect("unnamed = current");
        assert_eq!(none.name, "academy", "no name given → the caller's current room");
    }

    // what this catches: a room outside the caller's reach silently swapped for
    // the current one. It must be REFUSED, naming what is in reach.
    #[tokio::test]
    async fn a_room_the_caller_is_not_in_is_refused_with_the_rooms_in_reach() {
        let (_home, airc) = two_room_scope().await;
        let err = resolve_room(&airc, Some("somewhere-else"))
            .await
            .expect_err("an unsubscribed room is not resolvable from this handle");
        let text = err.to_string();
        assert!(text.contains("somewhere-else"), "names the room asked for: {text}");
        assert!(
            text.contains("#academy") && text.contains("#widgets"),
            "names the rooms that ARE in reach: {text}"
        );
    }
}
