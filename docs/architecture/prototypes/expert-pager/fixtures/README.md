# RUN-1 trace fixture (for BanditPlanController replay → pin doc)

- **`k3-routed-access.trace`** — live K3 GGML_MOE_TRACE_FILE slice. Binary, 12-byte
  records in access order: `tkey` (u64 LE) + `e` (u32 LE). ~84k records (~19 decode
  tokens; short — a longer slice can follow for the real datum).
  - `tkey` = FNV-1a of the canonical tensor name `blk.{layer}.ffn_{gate,up,down}_exps.weight`.
  - `e` = within-layer expert index.
  - **An expert = `(layer, e)`**; its 3 matrices (gate/up/down) share `e` but have 3
    distinct `tkey`s — dedup to (layer, e) for the pin_list.
- **`tkey-to-layer-matrix.json`** — reverse table `{ "<tkey>": {"layer":L,"matrix":"gate|up|down"} }`
  for blk 0..95. Maps each record's `tkey` → layer; `e` is the expert. No v2 record needed.

Emit self-check: `blk.5.ffn_up_exps.weight` → tkey `16542725649459479844`.
