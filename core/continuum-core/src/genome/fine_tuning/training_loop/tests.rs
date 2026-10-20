#![cfg(test)]
    use super::*;

    /// Deterministic fake tokenizer: maps each char to its ASCII
    /// value (mod a small vocab). pad_token_id = 0. Easy to reason
    /// about + reproducible.
    struct FakeTokenizer {
        vocab: u32,
    }

    impl Tokenizer for FakeTokenizer {
        fn encode(&self, text: &str) -> Result<Vec<u32>, TrainingError> {
            Ok(text
                .bytes()
                .map(|b| (b as u32 % self.vocab).max(1)) // avoid colliding with pad=0
                .collect())
        }
        fn pad_token_id(&self) -> u32 {
            0
        }
    }

    fn example(prompt: &str, completion: &str) -> TrainingExample {
        TrainingExample {
            prompt: prompt.into(),
            completion: completion.into(),
            metadata: None,
            lived: None,
        }
    }

    // what this catches: empty dataset → typed EmptyDataset, not a
    // mysterious tensor error later. The dead `channel.rs` trigger
    // we deleted in #1572 used to fire training on empty datasets;
    // this is the contract that catches "no examples sent" at the
    // boundary.
    #[test]
    fn empty_dataset_rejected() {
        let tok = FakeTokenizer { vocab: 32 };
        let err = DataLoader::new(&[], &tok, 1, 4, &Device::Cpu)
            .err()
            .expect("must reject");
        assert!(matches!(err, TrainingError::EmptyDataset));
    }

    // what this catches: 0 batch size → typed error. A future
    // refactor that silently sets batch_size=1 on zero input would
    // change training dynamics without the caller's knowledge.
    #[test]
    fn zero_batch_size_rejected() {
        let tok = FakeTokenizer { vocab: 32 };
        let err = DataLoader::new(&[example("hi", "ok")], &tok, 0, 4, &Device::Cpu)
            .err()
            .expect("must reject");
        assert!(matches!(err, TrainingError::InvalidBatchSize(0)));
    }

    // what this catches: 0 sequence length → typed error.
    #[test]
    fn zero_seq_length_rejected() {
        let tok = FakeTokenizer { vocab: 32 };
        let err = DataLoader::new(&[example("hi", "ok")], &tok, 1, 0, &Device::Cpu)
            .err()
            .expect("must reject");
        assert!(matches!(err, TrainingError::InvalidSequenceLength(0)));
    }

    // what this catches: batch tensor shape contract. Loader
    // produces `[batch_size, sequence_length]` shaped tensors for
    // input_ids, target_ids, attention_mask. A future refactor
    // that swaps axes (sequence-first) would break every
    // downstream consumer.
    #[test]
    fn batches_have_shape_batch_size_x_seq_len() {
        let tok = FakeTokenizer { vocab: 32 };
        let examples = vec![
            example("hello", "world"),
            example("foo", "bar"),
            example("ping", "pong"),
            example("aa", "bb"),
        ];
        let loader = DataLoader::new(&examples, &tok, 2, 6, &Device::Cpu).unwrap();
        // 4 examples / batch_size 2 = 2 batches.
        assert_eq!(loader.len(), 2);
        for batch in loader.batches() {
            assert_eq!(batch.input_ids.dims(), &[2, 6]);
            assert_eq!(batch.target_ids.dims(), &[2, 6]);
            assert_eq!(batch.attention_mask.dims(), &[2, 6]);
        }
    }

    // what this catches: attention_mask is 0 for pad positions, 1
    // for real tokens. A future refactor that flips polarity (1
    // for pad) would silently invert which positions contribute
    // to loss — training would treat pads as gradient signal.
    #[test]
    fn attention_mask_is_1_for_real_0_for_pad() {
        let tok = FakeTokenizer { vocab: 32 };
        let examples = vec![example("a", "b")]; // very short → padded
        let loader = DataLoader::new(&examples, &tok, 1, 8, &Device::Cpu).unwrap();
        let batch = loader.batches().next().unwrap();
        let mask: Vec<f32> = batch
            .attention_mask
            .flatten_all()
            .unwrap()
            .to_vec1()
            .unwrap();
        // First 2 tokens are real (encoded "ab"), rest are pad.
        assert_eq!(mask[0], 1.0);
        assert_eq!(mask[1], 1.0);
        for &v in &mask[2..] {
            assert_eq!(v, 0.0, "pad positions must have mask=0");
        }
    }

    // what this catches: partial last batch is dropped. The trainer
    // relies on stable batch_size for loss aggregation; a partial
    // final batch would skew metrics. A future refactor that pads
    // the partial would silently inject pad gradient.
    #[test]
    fn partial_last_batch_is_dropped() {
        let tok = FakeTokenizer { vocab: 32 };
        let examples = vec![example("a", "b"), example("c", "d"), example("e", "f")];
        // 3 examples, batch_size=2 → 1 full batch + 1 partial,
        // partial is dropped.
        let loader = DataLoader::new(&examples, &tok, 2, 4, &Device::Cpu).unwrap();
        assert_eq!(loader.len(), 1);
    }

    // what this catches: AdamW step actually moves the parameters.
    // After one train_step, at least one of A or B must have
    // changed (gradient flowed, optimizer updated). A future
    // refactor that misconfigures the var list (e.g. passes base
    // weight to AdamW instead of A/B) would silently freeze the
    // LoRA — the test catches that immediately.
    #[test]
    fn adamw_step_moves_lora_parameters() {
        let device = Device::Cpu;
        let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
        let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();

        // Capture A snapshot before training.
        let a_before: Vec<f32> = module
            .lora_a()
            .as_tensor()
            .flatten_all()
            .unwrap()
            .to_vec1()
            .unwrap();

        let mut trainer = LoRATrainer::new(module, 0.1).unwrap();

        // Build a fake batch with non-trivial inputs + targets so
        // the loss has gradient.
        let input_ids = Tensor::from_vec(vec![1u32, 2, 3, 0], (1, 4), &device).unwrap();
        let target_ids = Tensor::from_vec(vec![2u32, 3, 0, 0], (1, 4), &device).unwrap();
        let attn_mask = Tensor::full(1.0f32, (1, 4), &device).unwrap();
        // target_mask reflects target_ids pad status: [2, 3] non-pad
        // (1.0), [0, 0] pad (0.0). First-target IS non-pad here, so
        // train_step gets a gradient-bearing step.
        let target_mask = Tensor::from_slice(&[1.0f32, 1.0, 0.0, 0.0], (1, 4), &device).unwrap();
        let batch = TokenizedBatch {
            input_ids,
            target_ids,
            attention_mask: attn_mask,
            target_mask,
        };

        trainer.train_step(&batch).unwrap();

        let a_after: Vec<f32> = trainer
            .module()
            .lora_a()
            .as_tensor()
            .flatten_all()
            .unwrap()
            .to_vec1()
            .unwrap();
        let max_delta = a_before
            .iter()
            .zip(a_after.iter())
            .map(|(b, a)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            max_delta > 0.0,
            "AdamW step must move A (gradient signal flowed); max |Δ| = {max_delta}"
        );
    }

    // what this catches: metrics counters increment correctly.
    // step counter advances by 1 per train_step; epoch counter
    // advances by 1 per train_epoch. A future refactor that
    // miscounts would make the lifecycle actor's progress_pct
    // wrong, surfacing as stuck status in operator dashboards.
    #[test]
    fn metrics_count_steps_and_epochs() {
        let device = Device::Cpu;
        let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
        let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();
        let mut trainer = LoRATrainer::new(module, 0.001).unwrap();

        // Vocab=4 lines up with the LoRAModule's out_features=4,
        // so the stand-in cross-entropy in train_step sees target
        // class indices that exist. A larger fake vocab would
        // produce out-of-range targets and surface as a gather
        // error inside cross_entropy — wrong path for THIS test
        // (this test exercises metric counters, not class-range
        // validation; that lives upstream in #233's real wiring).
        let tok = FakeTokenizer { vocab: 4 };
        let examples = vec![
            example("aa", "bb"),
            example("cc", "dd"),
            example("ee", "ff"),
            example("gg", "hh"),
        ];
        let loader = DataLoader::new(&examples, &tok, 2, 4, &device).unwrap();
        // 4 examples / batch=2 → 2 batches per epoch.
        assert_eq!(loader.len(), 2);

        assert_eq!(trainer.metrics().steps_completed, 0);
        assert_eq!(trainer.metrics().epochs_completed, 0);

        trainer.train_epoch(&loader).unwrap();
        assert_eq!(trainer.metrics().steps_completed, 2);
        assert_eq!(trainer.metrics().epochs_completed, 1);

        trainer.train_epoch(&loader).unwrap();
        assert_eq!(trainer.metrics().steps_completed, 4);
        assert_eq!(trainer.metrics().epochs_completed, 2);
    }

    /// VDD — validation-driven tests verifying numerical correctness
    /// against closed-form math + convergence invariants.
    ///
    /// Difference vs the TDD tests above: TDD pins CONTRACTS (error
    /// rejection, batch shape, metric counters). VDD pins NUMERICAL
    /// CORRECTNESS — the loss values match the log-sum-exp formula,
    /// and the optimizer actually converges on a known-easy problem.
    /// A refactor that swapped cross_entropy with a different
    /// reduction would pass TDD's "loss is a number" check; only
    /// VDD's formula match catches it.
    mod vdd {
        use super::*;

        // what this VDD catches: candle_nn::loss::cross_entropy
        // implements the standard formula
        //   CE(logits, target) = log(sum(exp(logits))) - logits[target]
        // for each sample, averaged over the batch. A refactor that
        // swapped log-softmax for plain softmax would silently change
        // the loss landscape and break gradient computation; this
        // test verifies candle's output against the formula computed
        // independently in f32.
        #[test]
        fn cross_entropy_matches_log_sum_exp_formula() {
            let device = Device::Cpu;
            // Two samples, three classes each. Targets: [0, 2].
            let logits_vec: Vec<f32> = vec![2.0, 1.0, 0.1, -0.5, 0.5, 1.5];
            let targets_vec: Vec<u32> = vec![0, 2];
            let logits = Tensor::from_slice(&logits_vec, (2, 3), &device).unwrap();
            let targets = Tensor::from_slice(&targets_vec, (2,), &device).unwrap();

            let loss = candle_nn::loss::cross_entropy(&logits, &targets).unwrap();
            let actual = loss.to_scalar::<f32>().unwrap();

            // Closed-form: average over samples of
            //   log(sum_j exp(z_j)) - z_target
            // Use log-sum-exp with max subtraction for numerical
            // stability, matching candle's path.
            let per_sample_loss = |row: &[f32], target: u32| -> f32 {
                let m = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                let lse = m + row.iter().map(|z| (z - m).exp()).sum::<f32>().ln();
                lse - row[target as usize]
            };
            let l0 = per_sample_loss(&logits_vec[0..3], targets_vec[0]);
            let l1 = per_sample_loss(&logits_vec[3..6], targets_vec[1]);
            let expected = (l0 + l1) / 2.0;

            let diff = (actual - expected).abs();
            assert!(
                diff < 1e-5,
                "VDD: cross_entropy mismatch — actual={actual:.6}, expected={expected:.6}, diff={diff:.2e}"
            );
        }

        // what this VDD catches: training a single example over many
        // steps must drive the loss down. This is the *convergence*
        // signal — the optimizer + loss + gradient pipeline all work
        // together. A refactor that broke gradient flow (e.g. forgot
        // to include B in AdamW's var list, or applied scale²
        // somewhere) would leave the loss flat. TDD's
        // adamw_step_moves_lora_parameters catches "some Δ"; VDD
        // catches "Δ in the *correct direction*" by checking that
        // repeated steps strictly reduce the loss on an overfit-able
        // single-example dataset.
        #[test]
        fn single_example_overfitting_strictly_decreases_loss() {
            let device = Device::Cpu;
            // Tiny module that can easily overfit one example.
            let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
            let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();
            // Higher LR than production defaults — we want clear
            // movement in a small step budget so the test stays fast.
            let mut trainer = LoRATrainer::new(module, 0.5).unwrap();

            // Single batch held constant across all steps. Both
            // masks set the SAME way (first target non-pad) so the
            // standin trains every step.
            let input_ids = Tensor::from_slice(&[1u32, 2, 3, 0], (1, 4), &device).unwrap();
            let target_ids = Tensor::from_slice(&[2u32, 3, 0, 0], (1, 4), &device).unwrap();
            let attn_mask = Tensor::full(1.0f32, (1, 4), &device).unwrap();
            let target_mask =
                Tensor::from_slice(&[1.0f32, 1.0, 0.0, 0.0], (1, 4), &device).unwrap();
            let batch = TokenizedBatch {
                input_ids,
                target_ids,
                attention_mask: attn_mask,
                target_mask,
            };

            let (loss_0, _) = trainer.train_step(&batch).unwrap();
            for _ in 0..40 {
                trainer.train_step(&batch).unwrap();
            }
            let (loss_final, _) = trainer.train_step(&batch).unwrap();

            // Convergence threshold: loss must drop to at most half
            // its initial value. Single-example overfitting on a
            // trainable cross-entropy with adequate steps SHOULD
            // drive loss to near zero; halving is a soft floor that
            // tolerates AdamW's first-step bias while still catching
            // any "loss flat or rising" regression.
            assert!(
                loss_final < loss_0 * 0.5,
                "VDD: convergence failed — loss_0={loss_0:.6}, loss_final={loss_final:.6}; \
                 single-example overfitting must strictly reduce loss"
            );

            // Belt-and-suspenders: the LAST step's loss must also be
            // below the FIRST step's. Avoid flaky outcomes where the
            // mid-training loss bounces but never converges below
            // start.
            assert!(
                loss_final < loss_0,
                "VDD: monotonicity — final loss {loss_final} must beat initial {loss_0}"
            );
        }

        // what this VDD catches: `gradient_tokens_consumed` counts
        // batch_size per step when ALL first-targets are non-pad
        // (the all-ones-mask common case). This is the honest
        // standin count. A regression flipping the mask polarity
        // or returning batch_size × seq_len from train_step would
        // here fail by a multiplicative factor.
        //
        // Per Reviewer 1's BLOCK M1: the load-bearing assertion is
        // "the count must equal what actually trained, not a
        // schedule-derived guess." With batch_size=2, all-ones
        // mask, 5 train_step calls → gradient_tokens_consumed = 10.
        //
        // Hand-built batch is fine HERE because both targets are
        // explicitly non-pad in target_ids. The drift-catching
        // version uses DataLoader; see `pad_first_target_via_dataloader`.
        #[test]
        fn gradient_tokens_consumed_counts_non_pad_first_targets() {
            let device = Device::Cpu;
            let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
            let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();
            let mut trainer = LoRATrainer::new(module, 0.001).unwrap();

            // batch_size=2, seq_len=4, both samples' first target
            // non-pad → tokens_used=2 per step.
            let input_ids =
                Tensor::from_slice(&[1u32, 2, 3, 0, 1, 2, 3, 0], (2, 4), &device).unwrap();
            let target_ids =
                Tensor::from_slice(&[2u32, 3, 0, 0, 2, 3, 0, 0], (2, 4), &device).unwrap();
            let attn_mask = Tensor::full(1.0f32, (2, 4), &device).unwrap();
            // target_mask matches target_ids: 1 where != 0, 0 where == 0.
            let target_mask = Tensor::from_slice(
                &[1.0f32, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0],
                (2, 4),
                &device,
            )
            .unwrap();
            let batch = TokenizedBatch {
                input_ids,
                target_ids,
                attention_mask: attn_mask,
                target_mask,
            };

            for _ in 0..5 {
                let (_, tokens_used) = trainer.train_step(&batch).unwrap();
                assert_eq!(
                    tokens_used, 2,
                    "VDD: each step trains batch_size=2 non-pad targets"
                );
            }
            assert_eq!(
                trainer.metrics().gradient_tokens_consumed,
                10,
                "VDD: accumulated honestly across 5 steps × batch=2"
            );
            // CRUCIALLY: NOT batch × seq × steps = 2 × 4 × 5 = 40.
            // The pre-fix formula was that inflation.
            assert_ne!(
                trainer.metrics().gradient_tokens_consumed,
                40,
                "VDD: the count must NOT be the inflated schedule-derived guess (2×4×5=40)"
            );
        }

        // what this VDD catches: short examples whose FIRST TARGET
        // is pad must NOT be counted as gradient-bearing. Per the
        // round-2 reviewer's BLOCK on the M1 semantic-drift bug:
        // the previous test hand-crafted a TokenizedBatch with mask
        // aligned to the implementation's BUG (first-INPUT pad
        // status), so the test passed but the bug shipped. The fix
        // is to build the batch VIA `DataLoader` — same path
        // production uses — so the test exercises real DataLoader
        // output. With a 1-byte example (prompt="X", completion="")
        // ByteTokenizer produces:
        //   inputs = [X+1, 0, 0, ...]
        //   target_ids = [0, 0, 0, ...]  (input shifted left)
        //   attention_mask = [1, 0, 0, ...]  (INPUT pad status)
        //   target_mask = [0, 0, 0, ...]  (TARGET pad status)
        // First INPUT is non-pad but first TARGET IS pad → the
        // metric MUST report 0 gradient-bearing tokens.
        //
        // Pre-fix this assertion would fire because the metric used
        // attention_mask[0]=1 → tokens_used=1, miscounting the
        // pad-targeted sample as gradient-bearing.
        #[test]
        fn pad_first_target_via_dataloader_contributes_zero_to_tokens_used() {
            use crate::genome::fine_tuning::byte_tokenizer::ByteTokenizer;
            let device = Device::Cpu;
            let base = Tensor::full(0.1f32, (257, 4), &device).unwrap();
            let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();
            let mut trainer = LoRATrainer::new(module, 0.001).unwrap();

            // Single 1-byte example. After encode + shift-left for
            // target_ids, every target position is pad.
            let examples = vec![TrainingExample {
                prompt: "X".into(),
                completion: "".into(),
                metadata: None,
                lived: None,
            }];
            let loader = DataLoader::new(&examples, &ByteTokenizer::new(), 1, 4, &device).unwrap();
            let batch = loader.batches().next().expect("one batch");

            let (_loss, tokens_used) = trainer.train_step(batch).unwrap();
            assert_eq!(
                tokens_used, 0,
                "VDD drift-pin: a 1-byte example produces an all-pad target_ids \
                 row → tokens_used MUST be 0. Pre-fix this returned 1 (counting \
                 input[0] pad status instead of target[0] pad status)."
            );
            // And `gradient_tokens_consumed` MUST NOT have moved.
            assert_eq!(
                trainer.metrics().gradient_tokens_consumed,
                0,
                "VDD: metric stays at 0 when every train_step's target_mask is empty"
            );
            // Step counter advances — train_step was called — but
            // the no-gradient branch was taken (no backward).
            assert_eq!(trainer.metrics().steps_completed, 1);
        }

        // what this VDD catches: a hand-built batch where ONE
        // sample's first target is non-pad and ANOTHER sample's
        // first target IS pad → tokens_used = 1 (only the non-pad).
        // This pins the per-sample counting math; the
        // DataLoader-based test above pins the DRIFT class.
        #[test]
        fn mixed_pad_and_real_first_targets_count_only_real() {
            let device = Device::Cpu;
            let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
            let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();
            let mut trainer = LoRATrainer::new(module, 0.001).unwrap();

            // batch_size=2; sample 0 first target=2 (non-pad),
            // sample 1 first target=0 (pad). target_mask reflects
            // that: [1, ..., 0, ...].
            let input_ids =
                Tensor::from_slice(&[1u32, 2, 3, 0, 0, 0, 0, 0], (2, 4), &device).unwrap();
            let target_ids =
                Tensor::from_slice(&[2u32, 3, 0, 0, 0, 0, 0, 0], (2, 4), &device).unwrap();
            let attn_mask = Tensor::from_slice(
                &[1.0f32, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                (2, 4),
                &device,
            )
            .unwrap();
            let target_mask = Tensor::from_slice(
                &[1.0f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                (2, 4),
                &device,
            )
            .unwrap();
            let batch = TokenizedBatch {
                input_ids,
                target_ids,
                attention_mask: attn_mask,
                target_mask,
            };
            let (_, tokens_used) = trainer.train_step(&batch).unwrap();
            assert_eq!(
                tokens_used, 1,
                "VDD: only the non-pad-first-target sample counts; pad sample contributes 0"
            );
        }

        // what this VDD catches: applying the masked-loss attention
        // mask is a no-op when the mask is all-1s (real positions
        // only). The stand-in trainer doesn't apply the mask at all
        // — but the test exists so the next slice (#233's real
        // wiring) maintains this invariant: an all-1s mask
        // produces the same loss as no mask. A future "improvement"
        // that multiplied by mask AND divided by sum(mask) without
        // guarding for the all-1s case would silently rescale the
        // loss landscape.
        #[test]
        fn all_ones_attention_mask_is_a_noop_relative_to_unmasked() {
            let device = Device::Cpu;
            let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
            let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();
            let mut trainer = LoRATrainer::new(module, 0.01).unwrap();

            let input_ids = Tensor::from_slice(&[1u32, 2, 3, 0], (1, 4), &device).unwrap();
            let target_ids = Tensor::from_slice(&[2u32, 3, 0, 0], (1, 4), &device).unwrap();
            let attn_all_ones = Tensor::full(1.0f32, (1, 4), &device).unwrap();
            let target_mask =
                Tensor::from_slice(&[1.0f32, 1.0, 0.0, 0.0], (1, 4), &device).unwrap();
            let batch_masked = TokenizedBatch {
                input_ids: input_ids.clone(),
                target_ids: target_ids.clone(),
                attention_mask: attn_all_ones,
                target_mask,
            };

            let (loss_masked, _) = trainer.train_step(&batch_masked).unwrap();
            // The standin doesn't apply mask, so this is a tautology
            // *today*. The assertion is a pin for the real wiring:
            // when mask multiplication lands, this test becomes
            // load-bearing.
            assert!(
                loss_masked.is_finite() && loss_masked > 0.0,
                "VDD: loss must be finite and positive on a meaningful target; got {loss_masked}"
            );
        }
    }
