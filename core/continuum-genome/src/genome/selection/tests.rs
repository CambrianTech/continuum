//! `select`'s contract: many genes by distance, weighted, held, with hysteresis.

use super::*;
use crate::genome::corpus_ref::CorpusRef;

const SPACE: &str = "space";

fn unit(v: [f32; 3]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter().map(|x| x / n).collect()
}

fn signature(centroid: [f32; 3], region: Option<f32>) -> GeneSignature {
    GeneSignature {
        embedder: SPACE.into(),
        dim: 3,
        centroid: unit(centroid),
        subspaces: vec![],
        corpus: CorpusRef { name: "t".into(), content_hash: "sha256:0".into(), size_bytes: 0, source_url: None },
        minted_at_ms: 0,
        parent: None,
        region,
    }
}

fn gene(name: &str) -> ActiveAdapterRequest {
    ActiveAdapterRequest { name: name.into(), path: format!("/genes/{name}.gguf"), domain: String::new(), scale: 1.0 }
}

fn pick<'a>(need: [f32; 3], genes: &[(&str, Option<&'a GeneSignature>)], previous: &[ActiveAdapterRequest]) -> Selection {
    let vector = unit(need);
    let need = Need { embedder_id: SPACE, vector: &vector, unrelated_null: None };
    let candidates: Vec<Candidate<'a>> = genes.iter().map(|(n, s)| Candidate { gene: gene(n), signature: *s }).collect();
    select(Some(&need), &candidates, previous)
}

fn scale_of(sel: &Selection, name: &str) -> f64 {
    sel.genes.iter().find(|g| g.name == name).map_or(0.0, |g| g.scale)
}

// what this catches: the one-gene `max_by` (rung 0) that the design replaced. Two genes the
// turn is inside BOTH apply, the nearer one heavier, as a convex mix; a gene the turn is far
// from stays off.
#[test]
fn two_near_genes_both_apply_weighted_by_depth_and_a_far_one_stays_off() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.8));
    let lex = signature([0.8, 0.6, 0.0], Some(0.8));
    let tax = signature([0.0, 0.0, 1.0], Some(0.8));
    let sel = pick([1.0, 0.3, 0.0], &[("parse", Some(&parse)), ("lex", Some(&lex)), ("tax", Some(&tax))], &[]);
    assert_eq!(sel.genes.len(), 2, "{sel:?}");
    let (p, l) = (scale_of(&sel, "parse"), scale_of(&sel, "lex"));
    assert!(p > l && l > 0.0, "nearer gene heavier: parse {p}, lex {l}");
    assert!((p + l - 1.0).abs() < 1e-6, "a convex mix: {p} + {l}");
    assert_eq!(sel.genes[0].name, "parse", "heaviest first");
    assert_eq!(scale_of(&sel, "tax"), 0.0);
    assert!(sel.changed);
}

// what this catches: a gene riding a turn outside every region. Nothing inside = the base
// alone (no `lora` scale for any gene), which the wire projects as explicit zeros.
#[test]
fn no_gene_inside_its_region_means_the_base_alone() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.9));
    let sel = pick([0.0, 1.0, 0.0], &[("parse", Some(&parse))], &[]);
    assert!(sel.genes.is_empty(), "{sel:?}");
    assert!(!sel.changed, "nothing ran before, nothing runs now");
}

// what this catches: thrash at a region's edge (each switch clears her KV). A gene that ran
// stays on between the leave and enter edges; one that did not run does not switch on there;
// below the leave edge it goes off.
#[test]
fn hysteresis_keeps_a_running_gene_between_the_edges_and_drops_it_past_leave() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.9)); // enter 0.9, leave 0.85
    let between = [0.875, (1.0f32 - 0.875 * 0.875).sqrt(), 0.0]; // similarity 0.875
    let off = pick(between, &[("parse", Some(&parse))], &[]);
    assert!(off.genes.is_empty(), "not running: it must cross the enter edge first");
    let mut running = gene("parse");
    running.scale = 1.0;
    let held = pick(between, &[("parse", Some(&parse))], std::slice::from_ref(&running));
    assert_eq!(scale_of(&held, "parse"), 1.0, "running: it stays until the leave edge");
    assert!(!held.changed, "same set, same scale: her KV is kept");
    let gone = pick([0.8, 0.6, 0.0], &[("parse", Some(&parse))], std::slice::from_ref(&running));
    assert!(gone.genes.is_empty(), "past the leave edge it goes off");
    assert!(gone.changed);
}

// what this catches: re-weighting every turn. The engine clears a slot's KV on ANY scale
// change, so while the applied set holds, the scales must be the previous turn's, bit for bit.
#[test]
fn weights_are_held_bit_identical_while_the_set_holds() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.8));
    let lex = signature([0.8, 0.6, 0.0], Some(0.8));
    let genes = [("parse", Some(&parse)), ("lex", Some(&lex))];
    let first = pick([1.0, 0.3, 0.0], &genes, &[]);
    let next = pick([1.0, 0.45, 0.0], &genes, &first.genes);
    assert_eq!(next.genes, first.genes, "the same set runs at the same scales");
    assert!(!next.changed);
}

// what this catches: a diffuse gene riding every turn. Its own region (0.3) is looser than
// the space's measured null + 2σ (0.5 + 2*0.1), so the null is the edge.
#[test]
fn the_measured_null_lifts_a_diffuse_genes_edge() {
    let diffuse = signature([1.0, 0.0, 0.0], Some(0.3));
    let vector = unit([0.6, 0.8, 0.0]); // similarity 0.6
    let candidates = [Candidate { gene: gene("diffuse"), signature: Some(&diffuse) }];
    let null = Need { embedder_id: SPACE, vector: &vector, unrelated_null: Some((0.5, 0.1)) };
    assert!(select(Some(&null), &candidates, &[]).genes.is_empty(), "inside its region, outside the null");
    let without = Need { unrelated_null: None, ..null };
    assert_eq!(select(Some(&without), &candidates, &[]).genes.len(), 1, "its own region alone admits it");
}

// what this catches: routing pretending to know. A gene with no signature, one signed in
// another space, or with neither a region nor a null cannot be routed: it runs at its trained
// scale as before routing, named unrouted. Without a need, routed genes stay as they were.
#[test]
fn unroutable_genes_run_at_their_trained_scale_and_no_need_keeps_the_genome() {
    let alien = GeneSignature { embedder: "other".into(), ..signature([1.0, 0.0, 0.0], Some(0.8)) };
    let edgeless = signature([1.0, 0.0, 0.0], None);
    let sel = pick([0.0, 1.0, 0.0], &[("plain", None), ("alien", Some(&alien)), ("edgeless", Some(&edgeless))], &[]);
    assert_eq!(sel.genes.len(), 3, "{sel:?}");
    assert!(sel.genes.iter().all(|g| g.scale == 1.0));
    assert!(sel.considered.iter().all(|c| c.route == Route::Unrouted));

    let parse = signature([1.0, 0.0, 0.0], Some(0.8));
    let mut ran = gene("parse");
    ran.scale = 0.625;
    let candidates = [Candidate { gene: gene("parse"), signature: Some(&parse) }];
    let kept = select(None, &candidates, std::slice::from_ref(&ran));
    assert_eq!(kept.genes, vec![ran]);
    assert!(!kept.changed);
}
