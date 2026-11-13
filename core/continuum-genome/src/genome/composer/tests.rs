//! The composer's contract: many genes by distance, weighted by depth, top-k, held, hysteresis.

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

fn ran(name: &str, scale: f64) -> ActiveAdapterRequest {
    ActiveAdapterRequest { scale, ..gene(name) }
}

fn pick_k<'a>(
    need: [f32; 3],
    genes: &[(&str, Option<&'a GeneSignature>)],
    capacity: usize,
    previous: &[ActiveAdapterRequest],
) -> Selection {
    let vector = unit(need);
    let need = Need { embedder_id: SPACE, vector: &vector, unrelated_null: None };
    let candidates: Vec<Candidate<'a>> = genes.iter().map(|(n, s)| Candidate { gene: gene(n), signature: *s }).collect();
    select(Some(&need), &candidates, capacity, previous)
}

fn pick<'a>(need: [f32; 3], genes: &[(&str, Option<&'a GeneSignature>)], previous: &[ActiveAdapterRequest]) -> Selection {
    pick_k(need, genes, genes.len(), previous)
}

fn scale(sel: &Selection, name: &str) -> f64 {
    sel.phenotype.scale_of(&format!("/genes/{name}.gguf"))
}

// what this catches: the one-gene `max_by` (rung 0) and the every-gene-at-1.0 composer the
// design replaced. Two genes the turn is inside BOTH apply, the nearer one heavier, their scales
// a / max(1, Σa); a far gene stays off; one gene alone runs at its depth, never above 1.0.
#[test]
fn two_near_genes_both_apply_weighted_by_depth_and_a_far_one_stays_off() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.8));
    let lex = signature([0.8, 0.6, 0.0], Some(0.8));
    let tax = signature([0.0, 0.0, 1.0], Some(0.8));
    let sel = pick([1.0, 0.3, 0.0], &[("parse", Some(&parse)), ("lex", Some(&lex)), ("tax", Some(&tax))], &[]);
    assert_eq!(sel.phenotype.genes.len(), 2, "{sel:?}");
    let (p, l) = (scale(&sel, "parse"), scale(&sel, "lex"));
    assert!(p > l && l > 0.0, "nearer gene heavier: parse {p}, lex {l}");
    assert!((p + l - 1.0).abs() < 1e-6, "Σa > 1 here, so the scales sum to 1: {p} + {l}");
    assert_eq!(sel.phenotype.genes[0].name, "parse", "heaviest first");
    assert_eq!(scale(&sel, "tax"), 0.0);
    assert!(sel.changed);

    let alone = pick([1.0, 0.3, 0.0], &[("parse", Some(&parse))], &[]);
    let depth = (unit([1.0, 0.3, 0.0])[0] - 0.8) / 0.2;
    assert!((scale(&alone, "parse") - f64::from(depth)).abs() < 1e-6, "alone, a gene runs at its depth");
}

// what this catches: more genes on a turn than the engine holds. The `capacity` deepest win.
#[test]
fn only_the_capacity_deepest_genes_are_kept() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.7));
    let lex = signature([0.8, 0.6, 0.0], Some(0.7));
    let ast = signature([0.6, 0.8, 0.0], Some(0.7));
    let sel = pick_k([1.0, 0.3, 0.0], &[("parse", Some(&parse)), ("lex", Some(&lex)), ("ast", Some(&ast))], 2, &[]);
    let names: Vec<&str> = sel.phenotype.genes.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, vec!["parse", "lex"], "{sel:?}");
}

// what this catches: a gene riding a turn outside every region. Nothing inside = the base alone.
#[test]
fn no_gene_inside_its_region_means_the_base_alone() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.9));
    let sel = pick([0.0, 1.0, 0.0], &[("parse", Some(&parse))], &[]);
    assert!(sel.phenotype.genes.is_empty(), "{sel:?}");
    assert!(!sel.changed, "nothing ran before, nothing runs now");
}

// what this catches: thrash at a region's edge (each switch clears her KV). A gene that ran stays
// on between its floor and its leave edge; one that did not run does not switch on there; past
// the leave edge it goes off.
#[test]
fn hysteresis_keeps_a_running_gene_between_the_edges_and_drops_it_past_leave() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.9)); // floor 0.9, leave 0.85
    let between = [0.875, (1.0f32 - 0.875 * 0.875).sqrt(), 0.0]; // similarity 0.875
    assert!(pick(between, &[("parse", Some(&parse))], &[]).phenotype.genes.is_empty(), "it must cross its floor first");
    let running = ran("parse", 0.25);
    let held = pick(between, &[("parse", Some(&parse))], std::slice::from_ref(&running));
    assert_eq!(scale(&held, "parse"), 0.25, "running: it stays, at its held scale, until the leave edge");
    assert!(!held.changed, "same set, same scale: her KV is kept");
    let gone = pick([0.8, 0.6, 0.0], &[("parse", Some(&parse))], std::slice::from_ref(&running));
    assert!(gone.phenotype.genes.is_empty(), "past the leave edge it goes off");
    assert!(gone.changed);
}

// what this catches: re-weighting every turn. The engine clears a slot's KV on ANY scale change,
// so while the applied set holds, the scales are the previous turn's, bit for bit.
#[test]
fn weights_are_held_bit_identical_while_the_set_holds() {
    let parse = signature([1.0, 0.0, 0.0], Some(0.8));
    let lex = signature([0.8, 0.6, 0.0], Some(0.8));
    let genes = [("parse", Some(&parse)), ("lex", Some(&lex))];
    let first = pick([1.0, 0.3, 0.0], &genes, &[]);
    let next = pick([1.0, 0.45, 0.0], &genes, &first.phenotype.genes);
    assert_eq!(next.phenotype, first.phenotype, "the same set runs at the same scales");
    assert!(!next.changed);
}

// what this catches: a diffuse gene riding every turn. Its own region (0.3) is looser than the
// space's measured null + 2σ (0.5 + 2*0.1), so the null is the floor.
#[test]
fn the_measured_null_lifts_a_diffuse_genes_floor() {
    let diffuse = signature([1.0, 0.0, 0.0], Some(0.3));
    let vector = unit([0.6, 0.8, 0.0]); // similarity 0.6
    let candidates = [Candidate { gene: gene("diffuse"), signature: Some(&diffuse) }];
    let null = Need { embedder_id: SPACE, vector: &vector, unrelated_null: Some((0.5, 0.1)) };
    assert!(select(Some(&null), &candidates, 1, &[]).phenotype.genes.is_empty(), "inside its region, outside the null");
    let without = Need { unrelated_null: None, ..null };
    assert_eq!(select(Some(&without), &candidates, 1, &[]).phenotype.genes.len(), 1, "its own region alone admits it");
}

// what this catches: routing pretending to know. A gene with no signature, one signed in another
// space, or with neither a region nor a null cannot be routed: it runs at its trained scale as
// before routing, named unrouted. Without a perception, routed genes stay as they were.
#[test]
fn unroutable_genes_run_at_their_trained_scale_and_no_perception_keeps_the_genome() {
    let alien = GeneSignature { embedder: "other".into(), ..signature([1.0, 0.0, 0.0], Some(0.8)) };
    let edgeless = signature([1.0, 0.0, 0.0], None);
    let sel = pick([0.0, 1.0, 0.0], &[("plain", None), ("alien", Some(&alien)), ("edgeless", Some(&edgeless))], &[]);
    assert_eq!(sel.phenotype.genes.len(), 3, "{sel:?}");
    assert!(sel.phenotype.genes.iter().all(|g| g.scale == 1.0));
    assert!(sel.considered.iter().all(|c| c.route == Route::Unrouted));

    let parse = signature([1.0, 0.0, 0.0], Some(0.8));
    let candidates = [Candidate { gene: gene("parse"), signature: Some(&parse) }];
    let before = ran("parse", 0.625);
    let kept = select(None, &candidates, 1, std::slice::from_ref(&before));
    assert_eq!(kept.phenotype.genes, vec![before]);
    assert!(!kept.changed);
}

// what this catches: an examiner's P ∖ g that moves the other genes (renormalized), which would
// credit g with the others' change. Only g goes to 0.
#[test]
fn without_one_gene_leaves_every_other_scale_unchanged() {
    let p = Phenotype { genes: vec![ran("parse", 0.6), ran("lex", 0.4)] };
    let less = p.without("/genes/parse.gguf");
    assert_eq!(less.genes, vec![ran("lex", 0.4)]);
    assert_eq!(less.scale_of("/genes/parse.gguf"), 0.0);
    assert_eq!(p.without("/genes/absent.gguf"), p);
}
