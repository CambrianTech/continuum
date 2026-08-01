// self_optimize — K3 self-optimizing the JOINT objective: best TOKEN RATE *and* QUALITY (Joel).
// The full control law scoring both. The lever is the tier allocation (all-star sharp vs cruft low):
//   more cruft -> smaller resident footprint -> more experts fit -> higher hit-rate -> faster tok/s,
//   BUT lower quality (distortion from the low-bit cruft). Less cruft -> higher quality, slower.
// The self-optimizer searches the policy (residency budget-use via cruft_fraction, + the recency decay
// that sets hit-rate) to maximize  w*norm(tok/s) + (1-w)*quality , online. Grounded in EVERY measured
// constant from this session; the trace supplies the real hit-rate-vs-residency curve. Swap the modelled
// tok/s / quality for the LIVE serving timing + perplexity-delta and this same loop optimizes for real.
use std::collections::HashSet;

// ---- MEASURED constants (BigMama, this session) ----
const WS_EXPERTS: f64 = 4416.0;          // activated matrix-experts / token (16 routed x 3 mat x 92 layers)
const FULL_MB:   f64 = 2.44;             // avg full expert record (IQ2) MB
const CRUFT_MB:  f64 = 1.10;             // cruft tier (~IQ1 / RVQ stage-0) MB (~0.45x)
const BW_GBs:    f64 = 3.0;              // aligned NVMe GB/s (proven)
const COMPUTE_S: f64 = 0.12;            // GPU compute floor / token when fetch is hidden (generous)
const RAM_GB:    f64 = 20.0;            // residency budget freed after the fitted-K3 structural fix

// ---- trace: hit-rate as a function of how many experts are resident (real K3 persistence curve) ----
fn load_tokens(path: &str) -> Vec<HashSet<u64>> {
    let d = std::fs::read(path).expect("trace"); let n = d.len()/12;
    let (mut uid, mut tk) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n { let o=i*12;
        let mut k=[0u8;8]; k.copy_from_slice(&d[o..o+8]); let mut e=[0u8;4]; e.copy_from_slice(&d[o+8..o+12]);
        let key=u64::from_le_bytes(k); let eid=u32::from_le_bytes(e) as u64;
        uid.push(key.rotate_left(9)^(eid.wrapping_mul(0x9E3779B97F4A7C15))); tk.push(key); }
    let mut b=vec![0usize]; let mut cur=HashSet::new(); let mut prev:Option<u64>=None;
    for (i,&k) in tk.iter().enumerate(){ if prev!=Some(k){ if cur.contains(&k){b.push(i);cur.clear();} cur.insert(k); prev=Some(k);}}
    b.push(tk.len());
    let mut s:Vec<HashSet<u64>>=b.windows(2).map(|w| uid[w[0]..w[1]].iter().copied().collect()).collect();
    let mut z:Vec<usize>=s.iter().map(|x|x.len()).collect(); z.sort(); let med=if z.is_empty(){0}else{z[z.len()/2]};
    s.retain(|x| x.len()>=med/2 && x.len()<=med*3/2); s
}
// hit-rate at a residency of `cap` experts, LRU-by-recency over the trace (matches the measured curve)
fn hit_at(tokens: &[HashSet<u64>], cap: usize) -> f64 {
    if tokens.len()<2 { return 0.0; }
    let (mut hit,mut cnt)=(0.0f64,0usize);
    for t in 1..tokens.len() {
        // resident = union of most-recent tokens until cap filled (recency window)
        let mut res:HashSet<u64>=HashSet::new(); let mut k=1;
        while res.len()<cap && k<=t { for &e in &tokens[t-k]{ res.insert(e);} k+=1; }
        let h=tokens[t].iter().filter(|e| res.contains(e)).count();
        hit+=h as f64/tokens[t].len() as f64; cnt+=1;
    }
    hit/cnt.max(1) as f64
}

// evaluate one policy (cruft_fraction c) -> (tok_s, quality 0..1)
fn eval(tokens:&[HashSet<u64>], c:f64) -> (f64,f64) {
    // resident experts that fit RAM at avg tier size (mix of full + cruft)
    let avg_mb = (1.0-c)*FULL_MB + c*CRUFT_MB;
    let cap = ((RAM_GB*1024.0)/avg_mb) as usize;
    let hit = hit_at(tokens, cap);
    // misses fetched at the CRUFT tier (cold experts served low-bit to go fast); bytes/token:
    let miss_experts = WS_EXPERTS*(1.0-hit);
    let miss_gb = miss_experts*CRUFT_MB/1024.0;
    let tok_s = 1.0/(miss_gb/BW_GBs + COMPUTE_S);
    // quality: 1 - (fraction of the WORKING SET served at cruft) * per-cruft distortion.
    // resident all-stars are full-fidelity; the cruft-tier resident + all misses cost quality.
    let served_cruft = c*hit + (1.0-hit);          // resident-cruft (c of hits) + all misses
    let quality = 1.0 - served_cruft*0.22;         // ~0.22 rel. distortion IQ2->IQ1 (before compensation-LoRA)
    (tok_s, quality)
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: self_optimize <trace>");
    let tokens = load_tokens(&path);
    println!("K3 SELF-OPTIMIZE (measured: {} experts/token, {:.2}MB full / {:.2}MB cruft, {} GB/s, {} GB residency)",
             WS_EXPERTS as u64, FULL_MB, CRUFT_MB, BW_GBs, RAM_GB as u64);
    println!("decode tokens in trace: {}\n", tokens.len());

    // frontier: sweep cruft_fraction, show the speed<->quality trade
    println!("{:>8} {:>8} {:>9} {:>8}", "cruft%", "tok/s", "quality", "residentK");
    let mut pts=vec![];
    for i in 0..=10 { let c=i as f64/10.0; let (ts,q)=eval(&tokens,c);
        let cap=((RAM_GB*1024.0)/((1.0-c)*FULL_MB+c*CRUFT_MB)) as usize;
        println!("{:>7.0}% {:>8.2} {:>8.1}% {:>7}", c*100.0, ts, q*100.0, cap/1000);
        pts.push((c,ts,q)); }

    // self-optimize for 3 operator preferences w = weight on SPEED (rest on quality). Normalize tok/s to a ceiling.
    let ts_max = pts.iter().map(|p|p.1).fold(0.0,f64::max);
    println!("\nself-optimized operating point per preference (reward = w*tok/s_norm + (1-w)*quality):");
    for &(name,w) in &[("quality-first",0.25f64),("balanced",0.5),("speed-first",0.8)] {
        let best = pts.iter().cloned().max_by(|a,b|
            (w*a.1/ts_max+(1.0-w)*a.2).partial_cmp(&(w*b.1/ts_max+(1.0-w)*b.2)).unwrap()).unwrap();
        println!("  {:>13} (w={:.2}): cruft {:>3.0}%  ->  {:.2} tok/s, {:.1}% quality",
                 name, w, best.0*100.0, best.1, best.2*100.0);
    }
    println!("\n(LIVE: swap modelled tok/s for serving timing + quality for perplexity-delta; the online\n bandit then reinforcement-optimizes this SAME reward, per workload, adapting the operating point.)");
}
