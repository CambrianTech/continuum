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
        names.push(room.name);
    }
    Err(CommandError::Invalid(not_in_reach(named, wanted, &names)))
}

/// PURE: the refusal for a room the caller is not in, leading with the room she most likely
/// meant. Kimi sent `swe-bench` for `#bench-swe-bench-verified-1789170698` three times in an
/// hour (2026-10-06, QA 598de62b): the list held the answer, but nothing pointed at it.
fn not_in_reach(named: &str, wanted: &str, names: &[String]) -> String {
    let listed = names.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ");
    let lower = wanted.to_lowercase();
    let containing: Vec<&String> = if lower.len() >= 3 {
        names.iter().filter(|n| n.to_lowercase().contains(&lower)).collect()
    } else {
        Vec::new() // two letters are inside half the room names: a listing is more honest
    };
    let meant = match containing.as_slice() {
        [one] => Some(format!("did you mean #{one}? ")),
        [] => crate::code::path_security::nearest_name(wanted, names).map(|n| format!("did you mean #{n}? ")),
        many => Some(format!(
            "rooms with {wanted:?} in their name: {}. ",
            many.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ")
        )),
    };
    format!(
        "room {named:?} is not among the rooms this caller is in: {}name one of these ({listed}), or join it first (room/join)",
        meant.unwrap_or_default()
    )
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

    // what this catches: the refusal listing the right room without pointing at it
    // (Kimi's `swe-bench` for the bench room, QA 598de62b). A name she typed that is
    // INSIDE exactly one room's name, or a typo or two off one, is named first; several
    // containing rooms are named together; nothing close names nothing.
    #[test]
    fn a_refused_room_leads_with_the_one_she_meant() {
        let names: Vec<String> =
            ["general", "bench-swe-bench-verified-1789170698", "bench-humaneval-1789170001", "academy"]
                .map(String::from)
                .to_vec();
        let lead = |want: &str| not_in_reach(want, want.trim_start_matches('#'), &names);

        let contained = lead("swe-bench");
        assert!(contained.contains("did you mean #bench-swe-bench-verified-1789170698?"), "{contained}");
        assert!(contained.contains("#general"), "still lists every room in reach: {contained}");

        let several = lead("bench");
        assert!(
            several.contains("rooms with \"bench\" in their name: #bench-swe-bench-verified-1789170698, #bench-humaneval-1789170001"),
            "{several}"
        );

        assert!(lead("#acadmy").contains("did you mean #academy?"), "a typo resolves by edit distance");
        assert!(!lead("default").contains("did you mean"), "nothing close: no guess");
        assert!(!lead("ge").contains("did you mean"), "too short to contain-match");
    }
}
