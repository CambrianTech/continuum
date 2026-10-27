#![cfg(test)]
//! A resident's finish against the real airc board (card 4356a90f): it waits in Review for a
//! linked PR's merge, and never undoes a merge once it lands.

use std::sync::Arc;

use airc_work::model::CardState;

// what this catches (card 4356a90f, Codex on #5399): Kimi closed cards right after their passes,
// before their PRs merged, stranding airc work merge (it needs Review). On a real board, her close
// on a card with a linked, unmerged PR lands in Review; once the merge is attested the card is
// Merged, and her later closes, repeated, keep it Merged rather than downgrading it to Closed,
// where the next close would reopen it.
#[tokio::test]
async fn her_close_waits_for_the_merge_and_repeated_closes_never_undo_it() -> Result<(), Box<dyn std::error::Error>> {
    use crate::persona::work_board::advance::advance_card_state_effective;
    let home = tempfile::tempdir()?;
    let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
    let airc = Arc::new(airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path()).await?);
    let room = airc.join("finish-awaits-merge").await?;
    let her = Some(airc.peer_id().as_uuid());
    let repo = airc_work::RepoId::new("acme/ordinary-project")?;
    let card = airc
        .create_work_card(airc_lib::CreateWorkCard::new(repo.clone(), "ordinary work with a PR", airc_lib::Priority::P1))
        .await?;
    airc.claim_work_card(airc_lib::ClaimWorkCard { card_id: card, ttl_ms: 600_000 }).await?;
    let pull_request = airc_work::model::PullRequestRef {
        repo,
        number: 1,
        head: airc_work::model::BranchName::new("ordinary-work")?,
        base: airc_work::model::BranchName::new("canary")?,
    };
    airc.link_card_pull_request(airc_lib::LinkCardPullRequest { card_id: card, pull_request: pull_request.clone() })
        .await?;
    // Before the merge: her close waits in Review.
    let landed = advance_card_state_effective(&airc, card, CardState::Closed, "work-state-verb", her).await?;
    assert_eq!(landed, CardState::Review);
    assert_eq!(airc.work_board_in(&room).await?.card(card).map(|c| c.state.clone()), Some(CardState::Review), "the board holds it in Review");

    // The merge lands.
    airc.mark_pull_request_merged(airc_lib::MarkPullRequestMerged {
        card_id: card,
        pull_request,
        merged_at_ms: continuum_base::time::EpochMs::now().millis(),
    })
    .await?;
    assert_eq!(airc.work_board_in(&room).await?.card(card).map(|c| c.state.clone()), Some(CardState::Merged));

    // After it: her closes, repeated, keep it Merged.
    for _ in 0..2 {
        let landed = advance_card_state_effective(&airc, card, CardState::Closed, "work-state-verb", her).await?;
        assert_eq!(landed, CardState::Merged);
        assert_eq!(
            airc.work_board_in(&room).await?.card(card).map(|c| c.state.clone()),
            Some(CardState::Merged),
            "a merge is never undone by her close"
        );
    }
    Ok(())
}
