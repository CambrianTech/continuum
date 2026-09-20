// predictor — v2 policy: a LEARNED residency predictor trained on the real K3 access trace ("train from
// the simulation"). Per-expert EMA activation score with a LEARNED decay (decay->0 = pure recency /
// last-token; decay->1 = pure frequency / LFU; the sweet spot in between is what we learn). Each token
// the resident set = the top-B experts by score. We TRAIN the decay on the early tokens and REPORT the
// hit-rate on held-out later tokens, vs the recency-window (last-K) and LFU and Belady-OPT baselines.
// std-only Rust; this is the ServingExpertPager v2 policy in miniature (the seam's swappable body).
use std::collections::{HashMap, HashSet};

fn load(path: &str) -> (Vec<u64>, Vec<u64>) {
    let d = std::fs::read(path).expect("trace");
    let n = d.len() / 12;
    let (mut uid, mut tk) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let o = i * 12;
        let mut k = [0u8; 8]; k.copy_from_slice(&d[o..o + 8]);
        let key = u64::from_le_bytes(k);
        let mut e = [0u8; 4]; e.copy_from_slice(&d[o + 8..o + 12]);
        let eid = u32::from_le_bytes(e) as u64;
        uid.push(key.rotate_left(9) ^ (eid.wrapping_mul(0x9E3779B97F4A7C15)));
        tk.push(key);
    }
    (uid, tk)
}

// token boundaries: a tensor key repeats 16x within its group; a boundary is when a key ALREADY seen
// this token reappears (cycle wrapped). Then keep only DECODE tokens (modal size band).
fn decode_tokens(uid: &[u64], tk: &[u64]) -> Vec<Vec<u64>> {
    let mut bounds = vec![0usize];
    let mut cur: HashSet<u64> = HashSet::new();
    let mut prev: Option<u64> = None;
    for (i, &k) in tk.iter().enumerate() {
        if prev != Some(k) {
            if cur.contains(&k) { bounds.push(i); cur.clear(); }
            cur.insert(k); prev = Some(k);
        }
    }
    bounds.push(tk.len());
    let mut sets: Vec<Vec<u64>> = Vec::new();
    for w in bounds.windows(2) { sets.push(uid[w[0]..w[1]].to_vec()); }
    let mut sizes: Vec<usize> = sets.iter().map(|s| s.len()).collect(); sizes.sort();
    let med = sizes[sizes.len() / 2];
    sets.into_iter().filter(|s| s.len() >= med / 2 && s.len() <= med * 3 / 2).collect()
}

// EMA predictor: hit-rate over a token RANGE at a given decay + budget B. Predicts token t (t in range)
// from scores accumulated over tokens < t; then updates scores with token t.
fn ema_hitrate(tokens: &[HashSet<u64>], decay: f64, budget: usize, range: std::ops::Range<usize>) -> f64 {
    let mut score: HashMap<u64, f64> = HashMap::new();
    let (mut hit, mut cnt) = (0.0f64, 0usize);
    for (t, tok) in tokens.iter().enumerate() {
        if range.contains(&t) && t > 0 {
            // resident = top-B by score
            let mut v: Vec<(f64, u64)> = score.iter().map(|(&k, &s)| (s, k)).collect();
            let b = budget.min(v.len());
            if b == 0 { continue; }
            let idx = (b - 1).min(v.len() - 1);
            v.select_nth_unstable_by(idx, |a, c| c.0.partial_cmp(&a.0).unwrap());
            let resident: HashSet<u64> = v[..b].iter().map(|x| x.1).collect();
            let h = tok.iter().filter(|e| resident.contains(e)).count();
            hit += h as f64 / tok.len() as f64; cnt += 1;
        }
        for s in score.values_mut() { *s *= decay; }
        for &e in tok { *score.entry(e).or_insert(0.0) += 1.0; }
    }
    if cnt > 0 { hit / cnt as f64 * 100.0 } else { 0.0 }
}

fn persistence(tokens: &[HashSet<u64>], k: usize, range: std::ops::Range<usize>) -> (f64, f64) {
    let (mut hit, mut cnt, mut usz) = (0.0f64, 0usize, 0.0f64);
    for t in range {
        if t < k { continue; }
        let mut u: HashSet<u64> = HashSet::new();
        for j in 1..=k { u.extend(tokens[t - j].iter().copied()); }
        let h = tokens[t].iter().filter(|e| u.contains(e)).count();
        hit += h as f64 / tokens[t].len() as f64; usz += u.len() as f64; cnt += 1;
    }
    if cnt > 0 { (hit / cnt as f64 * 100.0, usz / cnt as f64) } else { (0.0, 0.0) }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: predictor <trace>");
    let (uid, tk) = load(&path);
    let toks_v = decode_tokens(&uid, &tk);
    let tokens: Vec<HashSet<u64>> = toks_v.iter().map(|s| s.iter().copied().collect()).collect();
    let tokens = &tokens[..];
    let n = tokens.len();
    println!("decode tokens: {}  (modal size ~{})", n, tokens.get(0).map(|s| s.len()).unwrap_or(0));
    if n < 4 { println!("too few decode tokens to train/test"); return; }
    let split = n / 2;                      // train on [1,split), test on [split,n)
    let budget = tokens[1.min(n-1)].len() * 3 / 2;   // ~1.5 tokens' worth of experts resident
    println!("train tokens 1..{}, TEST {}..{}, budget {} experts (~1.5 tokens)\n", split, split, n, budget);

    // TRAIN: sweep decay on the train range, pick the best
    let decays = [0.0, 0.3, 0.5, 0.7, 0.85, 0.9, 0.95, 0.99];
    let mut best = (-1.0f64, 0.0f64);
    println!("{:>7} {:>12}", "decay", "train hit%");
    for &d in &decays {
        let h = ema_hitrate(tokens, d, budget, 1..split);
        println!("{:>7.2} {:>11.1}%", d, h);
        if h > best.0 { best = (h, d); }
    }
    let learned_decay = best.1;
    println!("\n=> learned decay = {:.2} (train {:.1}%)\n", learned_decay, best.0);

    // TEST (held-out): learned predictor vs baselines
    let ema_test = ema_hitrate(tokens, learned_decay, budget, split..n);
    let (p1, _) = persistence(tokens, 1, split..n);
    let (p2, _) = persistence(tokens, 2, split..n);
    let lfu_test = ema_hitrate(tokens, 0.99, budget, split..n);   // decay~1 = frequency = LFU-ish
    let rec_test = ema_hitrate(tokens, 0.0, budget, split..n);    // decay=0 = pure last-token recency
    println!("HELD-OUT hit-rate (test tokens {}..{}, budget {} experts):", split, n, budget);
    println!("  LEARNED EMA (decay {:.2}) : {:.1}%", learned_decay, ema_test);
    println!("  pure recency (decay 0)   : {:.1}%", rec_test);
    println!("  pure frequency (decay .99): {:.1}%", lfu_test);
    println!("  persistence last-1 token : {:.1}%", p1);
    println!("  persistence last-2 tokens: {:.1}%", p2);
}
