pub async fn advance_card_state_effective(
    airc: &Arc<airc_lib::Airc>,
    card_id: String,
    desired_state: CardState,
    verb: &str,
    resident: Option<String>,
) -> Result<CardState, Box<dyn std::error::Error>> { ... }
