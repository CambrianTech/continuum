#![cfg(test)]
use super::*;

fn gate() -> &'static LaneAdmission {
    let gate = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(1);
    gate
}

#[tokio::test]
async fn unconsumed_scope_releases_on_cancellation() -> Result<(), Box<dyn std::error::Error>> {
    let gate = gate();
    let lane = gate
        .acquire_serving_lane(None, LanePriority::Directed, None)
        .await;
    let mut scoped = Box::pin(with_serving_lane(lane, std::future::pending::<()>()));
    assert!(futures::poll!(&mut scoped).is_pending());
    let mut next = Box::pin(gate.acquire_serving_lane(None, LanePriority::Directed, None));
    assert!(futures::poll!(&mut next).is_pending());
    drop(scoped);
    drop(tokio::time::timeout(Duration::from_secs(2), next).await?);
    Ok(())
}

#[tokio::test]
async fn transfer_is_single_use_and_owner_outlives_scope() -> Result<(), Box<dyn std::error::Error>>
{
    let gate = gate();
    let lane = gate
        .acquire_serving_lane(None, LanePriority::Directed, None)
        .await;
    let owner = with_serving_lane(lane, async {
        let owner = take_serving_lane()?;
        assert!(
            take_serving_lane().is_err(),
            "a second consumer must not bypass the lane"
        );
        Ok::<_, String>(owner)
    })
    .await?
    .ok_or("transferred owner")?;
    let mut next = Box::pin(gate.acquire_serving_lane(None, LanePriority::Directed, None));
    assert!(futures::poll!(&mut next).is_pending());
    drop(owner);
    drop(tokio::time::timeout(Duration::from_secs(2), next).await?);
    Ok(())
}

#[tokio::test]
async fn nested_scope_does_not_replace_its_parent_custody() -> Result<(), Box<dyn std::error::Error>>
{
    let outer_gate = gate();
    let inner_gate = gate();
    let outer = outer_gate
        .acquire_serving_lane(None, LanePriority::Directed, None)
        .await;
    let inner = inner_gate
        .acquire_serving_lane(None, LanePriority::Directed, None)
        .await;
    with_serving_lane(outer, async {
        with_serving_lane(inner, async {
            drop(take_serving_lane()?);
            Ok::<_, String>(())
        })
        .await?;
        drop(take_serving_lane()?);
        Ok::<_, String>(())
    })
    .await?;
    drop(
        tokio::time::timeout(
            Duration::from_secs(2),
            outer_gate.acquire_serving_lane(None, LanePriority::Directed, None),
        )
        .await?,
    );
    drop(
        tokio::time::timeout(
            Duration::from_secs(2),
            inner_gate.acquire_serving_lane(None, LanePriority::Directed, None),
        )
        .await?,
    );
    assert!(
        take_serving_lane()?.is_none(),
        "no caller scope for adapter-owned traffic"
    );
    Ok(())
}
