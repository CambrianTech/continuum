//! The residency owner's contract: pinned genes come back, cold unpinned genes go first.

use super::*;

fn seat(id: i64, name: &str, on_device: bool, n_bytes: Option<u64>) -> GeneSeat {
    GeneSeat { id, path: format!("/g/{name}.gguf"), on_device, n_bytes }
}

fn path(name: &str) -> String {
    format!("/g/{name}.gguf")
}

fn phenotype(names: &[(&str, f64)]) -> Phenotype {
    Phenotype {
        genes: names
            .iter()
            .map(|(n, s)| ActiveAdapterRequest { name: n.to_string(), path: path(n), domain: String::new(), scale: *s })
            .collect(),
    }
}

// what this catches: a gene a mind runs left in host RAM, so every one of her requests pays
// the engine's on-demand page-in. The prefetch brings back exactly the pinned host genes.
#[test]
fn the_prefetch_pages_in_exactly_the_genes_minds_run() {
    let seats = [seat(0, "a", false, Some(9)), seat(1, "b", false, Some(9)), seat(2, "c", true, Some(9))];
    let pinned: BTreeSet<String> = [path("a"), path("c")].into_iter().collect();
    assert_eq!(prefetch(&seats, &pinned), vec![PageMove { id: 0, path: path("a"), to_device: true }]);
}

// what this catches: relieving the budget by paging out a gene a mind is running (her next
// request pages it straight back in), or by the hottest gene instead of the coldest. LRU-2: a
// gene used twice recently outranks one used once more recently; unknown sizes are never
// counted as freed bytes.
#[test]
fn victims_are_the_coldest_unpinned_genes_with_known_bytes() {
    let seats = [
        seat(0, "pinned", true, Some(100)),
        seat(1, "twice", true, Some(100)),
        seat(2, "once", true, Some(100)),
        seat(3, "never", true, Some(100)),
        seat(4, "unsized", true, None),
        seat(5, "host", false, Some(100)),
    ];
    let pinned: BTreeSet<String> = [path("pinned")].into_iter().collect();
    let mut heat = HashMap::new();
    heat.insert(path("twice"), Heat { last_ms: 50, prior_ms: 40, uses: 2 });
    heat.insert(path("once"), Heat { last_ms: 90, prior_ms: 0, uses: 1 });
    let (moves, freed) = victims(&seats, &pinned, &heat, 150);
    let ids: Vec<i64> = moves.iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![3, 2], "never-used first, then the once-used; the twice-used stays");
    assert_eq!(freed, 200);
    assert!(moves.iter().all(|m| !m.to_device));
    assert_eq!(victims(&seats, &pinned, &heat, 10_000).0.len(), 3, "never the pinned, unsized or host gene");
}

// what this catches: two copies of "what she runs": the composer's previous turn and the pins
// are one record, and a mind that runs nothing pins nothing.
#[test]
fn a_turn_pins_her_genes_and_is_her_previous_phenotype() {
    let owner = GeneResidency::default();
    let mind = Uuid::from_u128(7);
    owner.record_turn(mind, &phenotype(&[("a", 0.6), ("b", 0.4)]), 10);
    assert_eq!(owner.previous(mind), phenotype(&[("a", 0.6), ("b", 0.4)]).genes);
    assert_eq!(owner.state.lock().pinned().len(), 2);
    owner.record_turn(mind, &phenotype(&[]), 20);
    assert!(owner.previous(mind).is_empty());
    assert!(owner.state.lock().pinned().is_empty());
    assert_eq!(owner.state.lock().heat[&path("a")].uses, 1);
}
