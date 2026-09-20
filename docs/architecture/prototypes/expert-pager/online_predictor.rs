// online_predictor — the pager LEARNS FROM RUNNING IN THE SYSTEM (Joel). Offline sim is the warm start;
// this reinforcement-optimizes ONLINE. "Only requires scoring everything": the system emits the reward
// (realized hit-rate per token — a proxy for -fault_wait / +tok/s), and a bandit over the policy adapts
// to maximize it. Policy = per-expert EMA activation score; the ACTION is the decay (recency<->frequency
// dial). We run N decay candidates in parallel, each scored by its realized hit each token (a contextual
// multi-armed bandit); the running SYSTEM uses the current best-scoring arm, and the arm weights track
// the workload — so when locality shifts (non-stationary), the online learner follows where a fixed decay
// can't. std-only Rust; this is the ServingExpertPager's online-adaptation loop in miniature.
use std::collections::{HashMap, HashSet};

fn load(path: &str) -> (Vec<u64>, Vec<u64>) {
    let d = std::fs::read(path).expect("trace");
    let n = d.len() / 12;
    let (mut uid, mut tk) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let o = i * 12;
        let mut k = [0u8; 8]; k.copy_from_slice(&d[o..o + 8]);
        let mut e = [0u8; 4]; e.copy_from_slice(&d[o + 8..o + 12]);
        let key = u64::from_le_bytes(k); let eid = u32::from_le_bytes(e) as u64;
        uid.push(key.rotate_left(9) ^ (eid.wrapping_mul(0x9E3779B97F4A7C15))); tk.push(key);
    }
    (uid, tk)
}
fn decode_tokens(uid: &[u64], tk: &[u64]) -> Vec<HashSet<u64>> {
    let mut b = vec![0usize]; let mut cur = HashSet::new(); let mut prev: Option<u64> = None;
    for (i, &k) in tk.iter().enumerate() {
        if prev != Some(k) { if cur.contains(&k) { b.push(i); cur.clear(); } cur.insert(k); prev = Some(k); }
    }
    b.push(tk.len());
    let mut sets: Vec<HashSet<u64>> = b.windows(2).map(|w| uid[w[0]..w[1]].iter().copied().collect()).collect();
    let mut sz: Vec<usize> = sets.iter().map(|s| s.len()).collect(); sz.sort();
    let med = if sz.is_empty() { 0 } else { sz[sz.len()/2] };
    sets.retain(|s| s.len() >= med/2 && s.len() <= med*3/2); sets
}

// one policy arm: its own EMA score map at a fixed decay
struct Arm { decay: f64, score: HashMap<u64, f64>, reward: f64 }
impl Arm {
    fn new(decay: f64) -> Self { Arm { decay, score: HashMap::new(), reward: 0.0 } }
    fn predict(&self, budget: usize) -> HashSet<u64> {
        let mut v: Vec<(f64, u64)> = self.score.iter().map(|(&k,&s)|(s,k)).collect();
        let b = budget.min(v.len()); if b == 0 { return HashSet::new(); }
        let idx = (b-1).min(v.len()-1);
        v.select_nth_unstable_by(idx, |a,c| c.0.partial_cmp(&a.0).unwrap());
        v[..b].iter().map(|x| x.1).collect()
    }
    fn update(&mut self, tok: &HashSet<u64>) {
        for s in self.score.values_mut() { *s *= self.decay; }
        for &e in tok { *self.score.entry(e).or_insert(0.0) += 1.0; }
    }
}

// run: for each token predict (each arm), reward = realized hit; SYSTEM uses best-reward arm.
// returns (online-adaptive avg hit%, per-arm fixed avg hit%).
fn run(tokens: &[HashSet<u64>], budget: usize) -> (f64, Vec<(f64, f64)>) {
    let decays = [0.0f64, 0.3, 0.6, 0.85, 0.95, 0.99];
    let mut arms: Vec<Arm> = decays.iter().map(|&d| Arm::new(d)).collect();
    let alpha = 0.3;                     // reward-EMA rate (how fast the bandit adapts)
    let (mut online_hit, mut cnt) = (0.0f64, 0usize);
    let mut fixed_hit = vec![0.0f64; arms.len()];
    for (t, tok) in tokens.iter().enumerate() {
        if t > 0 {
            // pick the SYSTEM arm = current best realized reward (argmax; ties -> first)
            let best = arms.iter().enumerate().max_by(|a,b| a.1.reward.partial_cmp(&b.1.reward).unwrap()).unwrap().0;
            for (i, arm) in arms.iter_mut().enumerate() {
                let resident = arm.predict(budget);
                let h = tok.iter().filter(|e| resident.contains(e)).count() as f64 / tok.len() as f64;
                arm.reward = (1.0 - alpha) * arm.reward + alpha * h;   // SCORE EVERYTHING: realized hit is the reward
                fixed_hit[i] += h;
                if i == best { online_hit += h; }
            }
            cnt += 1;
        } else {
            for arm in arms.iter_mut() { let _ = arm.predict(budget); }  // warm
        }
        for arm in arms.iter_mut() { arm.update(tok); }
    }
    let per_arm = decays.iter().zip(fixed_hit.iter()).map(|(&d,&h)| (d, if cnt>0 { h/cnt as f64*100.0 } else {0.0})).collect();
    (if cnt>0 { online_hit/cnt as f64*100.0 } else {0.0}, per_arm)
}

// synthetic NON-STATIONARY trace: phase A strong recency (walk), phase B strong frequency (fixed hot set).
fn synth_nonstationary() -> Vec<HashSet<u64>> {
    let mut rng = 0x9E3779B97F4A7C15u64;
    let mut r = || { rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1); rng >> 20 };
    let per = 4000usize;
    let mut toks = Vec::new();
    // phase A: each token = 70% of previous token's experts + 30% fresh (recency dominates)
    let mut prev: Vec<u64> = (0..per as u64).collect();
    for _ in 0..15 {
        let mut s: HashSet<u64> = prev.iter().take(per*7/10).copied().collect();
        while s.len() < per { s.insert(1_000_000 + (r() % 2_000_000)); }
        prev = s.iter().copied().collect(); toks.push(s);
    }
    // phase B: 60% from a FIXED hot pool of 2*per + 40% random tail (frequency dominates)
    let hot: Vec<u64> = (5_000_000..5_000_000 + (per as u64*2)).collect();
    for _ in 0..15 {
        let mut s: HashSet<u64> = HashSet::new();
        while s.len() < per*6/10 { s.insert(hot[(r() as usize) % hot.len()]); }
        while s.len() < per { s.insert(9_000_000 + (r() % 5_000_000)); }
        toks.push(s);
    }
    toks
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (tokens, label) = if args.len() > 1 {
        let (uid, tk) = load(&args[1]); (decode_tokens(&uid, &tk), format!("real K3 trace ({})", args[1]))
    } else { (synth_nonstationary(), "synthetic NON-STATIONARY (recency->frequency shift)".to_string()) };
    let n = tokens.len();
    let budget = tokens.get(1).map(|s| s.len()).unwrap_or(4000) * 3 / 2;
    println!("== {} ==\ntokens: {}  budget: {} experts\n", label, n, budget);
    let (online, per_arm) = run(&tokens, budget);
    println!("{:>10} {:>10}", "decay", "fixed hit%");
    for (d, h) in &per_arm { println!("{:>10.2} {:>9.1}%", d, h); }
    let best_fixed = per_arm.iter().map(|x| x.1).fold(0.0, f64::max);
    println!("\n  ONLINE-ADAPTIVE (bandit) : {:.1}%", online);
    println!("  best FIXED decay          : {:.1}%", best_fixed);
    println!("  => online {} best-fixed by {:.1} pts", if online >= best_fixed {"MATCHES/beats"} else {"trails"}, (online-best_fixed).abs());
}
