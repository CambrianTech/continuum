// trace_replay — replay a real MoE expert-access trace against residency policies to measure the
// achievable hit-rate and activation skew. Decides whether a residency PREDICTOR can win (skewed reuse)
// or the box is working-set-bound (near-uniform => needs regional quant / grid). std-only Rust; the same
// replay+policy logic is what the production predictor in continuum-core's pager will run — no Python.
//
// Trace format: repeated (u64 tensor_key LE, u32 expert_id LE) — one record per activated expert, in
// access order (from GGML_MOE_TRACE_FILE). Unique expert identity = (tensor_key, expert_id).
use std::collections::{HashMap, HashSet, BinaryHeap};
use std::cmp::Reverse;

const AVG_BYTES: f64 = 2.6 * 1024.0 * 1024.0;

fn load(path: &str) -> (Vec<u64>, Vec<u64>) {
    let data = std::fs::read(path).expect("read trace");
    let rec = 12usize; // u64 + u32
    let n = data.len() / rec;
    let mut uid = Vec::with_capacity(n);
    let mut tkey = Vec::with_capacity(n);
    for i in 0..n {
        let o = i * rec;
        let mut k = [0u8; 8]; k.copy_from_slice(&data[o..o + 8]);
        let key = u64::from_le_bytes(k);
        let mut e = [0u8; 4]; e.copy_from_slice(&data[o + 8..o + 12]);
        let eid = u32::from_le_bytes(e) as u64;
        // combine to a single stable id (key is already a 64-bit hash; expert_id < 2^20)
        uid.push(key.rotate_left(9) ^ (eid.wrapping_mul(0x9E3779B97F4A7C15)));
        tkey.push(key);
    }
    (uid, tkey)
}

// token boundaries: each MoE tensor fires once per token but writes ONE record per selected expert, so a
// tensor_key repeats consecutively (16x) within its group. Collapse consecutive dups to the tensor
// sequence; a boundary is when a tensor key ALREADY seen this token reappears (the cycle wrapped).
fn token_boundaries(tkey: &[u64]) -> Vec<usize> {
    let mut b = vec![0usize];
    let mut cur: HashSet<u64> = HashSet::new();
    let mut prev: Option<u64> = None;
    for (i, &k) in tkey.iter().enumerate() {
        if prev != Some(k) {                 // new tensor group
            if cur.contains(&k) { b.push(i); cur.clear(); }
            cur.insert(k);
            prev = Some(k);
        }
    }
    b.push(tkey.len());
    b
}

fn sim_lru(seq: &[u64], cap: usize) -> f64 {
    let mut pos: HashMap<u64, u64> = HashMap::new(); // id -> last tick
    let mut order: std::collections::BTreeMap<u64, u64> = std::collections::BTreeMap::new(); // tick -> id
    let mut tick = 0u64; let mut hits = 0u64;
    for &x in seq {
        tick += 1;
        if let Some(&old) = pos.get(&x) { order.remove(&old); hits += 1; }
        else if pos.len() >= cap {
            if let Some((&t, &vid)) = order.iter().next() { order.remove(&t); pos.remove(&vid); }
        }
        pos.insert(x, tick); order.insert(tick, x);
    }
    hits as f64 / seq.len() as f64
}

fn sim_lfu(seq: &[u64], cap: usize) -> f64 {
    let mut freq: HashMap<u64, u64> = HashMap::new();
    let mut resident: HashSet<u64> = HashSet::new();
    let mut heap: BinaryHeap<Reverse<(u64, u64)>> = BinaryHeap::new(); // (freq, id) min-heap
    let mut hits = 0u64;
    for &x in seq {
        let f = freq.entry(x).or_insert(0); *f += 1; let nf = *f;
        if resident.contains(&x) { hits += 1; }
        else {
            if resident.len() >= cap {
                while let Some(Reverse((hf, hid))) = heap.pop() {
                    if resident.contains(&hid) && freq[&hid] == hf { resident.remove(&hid); break; }
                }
            }
            resident.insert(x);
        }
        heap.push(Reverse((nf, x)));
    }
    hits as f64 / seq.len() as f64
}

fn sim_opt(seq: &[u64], cap: usize) -> f64 {
    let n = seq.len();
    let mut next = vec![n; n];
    let mut last: HashMap<u64, usize> = HashMap::new();
    for i in (0..n).rev() { next[i] = *last.get(&seq[i]).unwrap_or(&n); last.insert(seq[i], i); }
    let mut resident: HashSet<u64> = HashSet::new();
    let mut heap: BinaryHeap<(usize, u64)> = BinaryHeap::new(); // (next_use, id) max-heap
    let mut hits = 0u64;
    for i in 0..n {
        let x = seq[i];
        if resident.contains(&x) { hits += 1; }
        else {
            if resident.len() >= cap {
                while let Some((nu, hid)) = heap.pop() {
                    if resident.contains(&hid) && nu >= i { resident.remove(&hid); break; }
                }
            }
            resident.insert(x);
        }
        heap.push((next[i], x));
    }
    hits as f64 / n as f64
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: trace_replay <file>");
    let (uid, tkey) = load(&path);
    let n = uid.len();
    let uniq: HashSet<u64> = uid.iter().copied().collect();
    let u = uniq.len();
    println!("accesses={}  unique_experts={}  ({:.1} GB if all resident)", n, u, u as f64 * AVG_BYTES / 1e9);

    // reuse vs horizon (does cross-token reuse climb with more tokens?)
    let b = token_boundaries(&tkey);
    let ntok = b.len() - 1;
    println!("\ntokens detected: {}", ntok);
    println!("{:>7} {:>10} {:>9} {:>18}", "tokens", "accesses", "unique", "repeat%(OPT ceil)");
    let mut probes: Vec<usize> = vec![2,4,8,12,16,24,32,48,ntok];
    probes.sort(); probes.dedup();
    for t in probes { if t < 1 || t > ntok { continue; }
        let pref = &uid[..b[t]];
        let uu: HashSet<u64> = pref.iter().copied().collect();
        let rep = (pref.len() - uu.len()) as f64 / pref.len() as f64 * 100.0;
        println!("{:>7} {:>10} {:>9} {:>17.1}%", t, pref.len(), uu.len(), rep);
    }

    // PERSISTENCE PREDICTOR: predict this token's active experts = last token's active set. Measures the
    // token-to-token overlap = recall of a trivial "keep last token resident" predictor. If high, a cheap
    // predictor beats LFU and enables prefetch (overlap I/O with compute). Reported per-token-pair, decode
    // only (skip the prefill union at token 0). Also last-K-token UNION recall (keep a rolling window).
    {
        let mut sets: Vec<HashSet<u64>> = Vec::new();
        for t in 0..ntok { sets.push(uid[b[t]..b[t+1]].iter().copied().collect()); }
        // DECODE tokens only: a single-token compute has ~4416 experts; prefill unions are much larger and
        // batched-imperfectly-segmented. Filter to the modal decode size band to measure clean persistence.
        let med = { let mut v: Vec<usize> = sets.iter().map(|s| s.len()).collect(); v.sort(); v[v.len()/2] };
        let decode: Vec<&HashSet<u64>> = sets.iter().filter(|s| s.len() <= med*3/2 && s.len() >= med/2).collect();
        println!("(decode tokens: {} of {} groups, modal size ~{})", decode.len(), ntok, med);
        if decode.len() >= 2 {
            for k in [1usize, 2, 4, 8] {
                if k >= decode.len() { continue; }
                let (mut rec, mut cnt) = (0.0f64, 0usize);
                for t in k..decode.len() {
                    let mut union: HashSet<u64> = HashSet::new();
                    for j in 1..=k { union.extend(decode[t-j].iter().copied()); }
                    let hit = decode[t].iter().filter(|e| union.contains(e)).count();
                    rec += hit as f64 / decode[t].len() as f64; cnt += 1;
                    let _ = union.len();
                }
                let avg = if cnt>0 { rec/cnt as f64*100.0 } else {0.0};
                let usz: f64 = { let mut s=0.0; for t in k..decode.len(){ let mut u=HashSet::new(); for j in 1..=k{u.extend(decode[t-j].iter().copied());} s+=u.len() as f64;} s/(decode.len()-k) as f64 };
                println!("persistence: keep last {:>1} token(s) ({:>6.0} experts, {:>4.1} GB) -> {:.1}% of next token already resident",
                    k, usz, usz*AVG_BYTES/1e9, avg);
            }
        }
    }

    // skew
    let mut freq: HashMap<u64, u64> = HashMap::new();
    for &x in &uid { *freq.entry(x).or_insert(0) += 1; }
    let mut fs: Vec<u64> = freq.values().copied().collect();
    fs.sort_unstable_by(|a, b| b.cmp(a));
    println!("\nskew (full trace):");
    for pct in [0.01, 0.05, 0.10, 0.25, 0.50] {
        let k = ((u as f64 * pct) as usize).max(1);
        let cov: u64 = fs[..k].iter().sum();
        println!("  top {:>4.0}% experts ({:>6}) cover {:>5.1}% of accesses", pct*100.0, k, cov as f64 / n as f64 * 100.0);
    }

    // policy hit-rate vs residency budget
    println!("\n{:>16} {:>6} {:>9} | {:>6} {:>6} {:>6}", "budget(experts)", "GB", "%resident", "LRU", "LFU", "OPT");
    for f in [0.02, 0.05, 0.10, 0.20, 0.35, 0.50, 0.75] {
        let cap = (u as f64 * f) as usize;
        if cap < 1 { continue; }
        println!("{:>16} {:>6.1} {:>8.0}% | {:>5.1}% {:>5.1}% {:>5.1}%",
            cap, cap as f64 * AVG_BYTES / 1e9, cap as f64 / u as f64 * 100.0,
            sim_lru(&uid, cap)*100.0, sim_lfu(&uid, cap)*100.0, sim_opt(&uid, cap)*100.0);
    }
}
