#![cfg(test)]
    use super::*;

    // what this catches: card 98c0ae23 — a core built LOCALLY at a docs- or baseline-only
    // commit above the build key was refused as a false DEPLOY MISMATCH (#5210 on the M5,
    // 10-19: stamped 29a474d34, key 9f97bc775), while a stamp whose build inputs differ
    // must still refuse.
    #[test]
    fn a_local_build_above_the_key_is_the_keys_core_but_a_source_change_is_not() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path();
        let git = |args: &[&str]| -> String {
            let out = std::process::Command::new("git")
                .current_dir(repo)
                .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"])
                .env_remove("GIT_DIR")
                .args(args)
                .output()
                .expect("git runs");
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        git(&["init", "-q"]);
        std::fs::create_dir_all(repo.join("core")).expect("mkdir");
        std::fs::write(repo.join("core/lib.rs"), "// core").expect("write");
        git(&["add", "."]);
        git(&["commit", "-qm", "core change"]);
        let key = git(&["rev-parse", "HEAD"]);
        std::fs::write(repo.join("README.md"), "docs only").expect("write");
        git(&["add", "."]);
        git(&["commit", "-qm", "docs only"]);
        let head = git(&["rev-parse", "HEAD"]);

        let (ci, _) = prebuilt_expectation(repo, None, &head, &key);
        assert!(sha_matches(&ci, &key), "a CI artifact at the key is expected as the key");
        let (local, source) = prebuilt_expectation(repo, None, &head, &head);
        assert!(sha_matches(&local, &head), "a local build at the docs-only HEAD is accepted: {source}");

        std::fs::write(repo.join("core/lib.rs"), "// core changed").expect("write");
        git(&["add", "."]);
        git(&["commit", "-qm", "core change 2"]);
        let newer = git(&["rev-parse", "HEAD"]);
        let (expected, _) = prebuilt_expectation(repo, None, &head, &newer);
        assert!(sha_matches(&expected, &key) && !sha_matches(&expected, &newer), "different build inputs still refuse");
    }

    // what this catches (M5, 2026-10-04 22:32-00:20Z): a consumer waiting on b85287366,
    // whose CI run was cancelled as superseded, while 3d81d1b09 was built and ready. A moved
    // request ends the wait; the same tip (short or long) and no request do not.
    #[test]
    fn a_moved_request_ends_the_wait_and_the_same_tip_does_not() {
        let waiting = "b852873662ecbeff1004f13b4fe929b9012ac2d6";
        assert_eq!(
            request_superseded(waiting, Some("3d81d1b09abc")),
            Some("3d81d1b09abc".into())
        );
        assert_eq!(
            request_superseded(waiting, Some("b85287366")),
            None,
            "the same tip, short"
        );
        assert_eq!(
            request_superseded(waiting, Some(waiting)),
            None,
            "the same tip, long"
        );
        assert_eq!(
            request_superseded(waiting, None),
            None,
            "no request = nothing newer asked for"
        );
    }

    const TIP: &str = "54cbe937f0123456789abcdef0123456789abcde";

    fn manifest() -> ArtifactManifest {
        ArtifactManifest {
            platform: "macos-x86_64".into(),
            git_sha: TIP.into(),
            features: "--features llama/mac-cpu-only,livekit-webrtc".into(),
            archive: "continuum-core-macos-x86_64.tar.gz".into(),
            sha256: "9e08699366264e94d11b6f51c545a5fa0a9a0d3254224cd1afc18c7d11898cea".into(),
            bins: REQUIRED_BINS.iter().map(|b| b.to_string()).collect(),
            cuda_version: None,
            cuda_compute_cap: None,
            runtime_libs: Vec::new(),
            engine: None,
        }
    }

    // what this catches (the 5090, 2026-10-06): a CUDA core staged onto a node whose driver
    // cannot load its runtime, or whose GPU is below its kernels' floor, dies at launch.
    // A build without CUDA needs nothing; the driver's major and the lowest GPU decide.
    #[test]
    fn a_cuda_build_runs_only_where_the_driver_and_gpus_can() {
        let banner = "| NVIDIA-SMI 580.97  Driver Version: 580.97  CUDA Version: 13.0 |";
        assert_eq!(driver_cuda(banner), Some((13, 0)));
        assert_eq!(driver_cuda("no gpu here"), None);
        assert_eq!(lowest_compute_cap("12.0\n8.6\n"), Some(86));
        assert_eq!(lowest_compute_cap(""), None);

        let cpu = manifest();
        assert!(
            gpu_verdict(&cpu, &NodeGpu::default()).is_ok(),
            "no CUDA, nothing to check"
        );

        let mut cuda = manifest();
        cuda.cuda_version = Some("12.9".into());
        cuda.cuda_compute_cap = Some(80);
        let the_5090 = NodeGpu {
            driver_cuda: Some((13, 0)),
            lowest_compute_cap: Some(120),
        };
        assert!(gpu_verdict(&cuda, &the_5090).is_ok());
        let old_driver = NodeGpu {
            driver_cuda: Some((11, 8)),
            ..the_5090.clone()
        };
        assert!(gpu_verdict(&cuda, &old_driver)
            .unwrap_err()
            .contains("update the NVIDIA driver"));
        // the same major is not enough: PTX from nvcc 12.9 does not JIT on a 12.4 driver
        let older_minor = NodeGpu {
            driver_cuda: Some((12, 4)),
            ..the_5090.clone()
        };
        assert!(gpu_verdict(&cuda, &older_minor)
            .unwrap_err()
            .contains("supports CUDA 12.4"));
        let same = NodeGpu {
            driver_cuda: Some((12, 9)),
            ..the_5090.clone()
        };
        assert!(gpu_verdict(&cuda, &same).is_ok());
        let old_gpu = NodeGpu {
            lowest_compute_cap: Some(75),
            ..the_5090.clone()
        };
        assert!(gpu_verdict(&cuda, &old_gpu)
            .unwrap_err()
            .contains("a GPU at 75"));
        assert!(
            gpu_verdict(&cuda, &NodeGpu::default()).is_err(),
            "unreadable is refused"
        );
    }

    // what this catches: a node taking a build that is not the program it would compile —
    // another commit, another platform, or the same commit with another GPU flavor's features.
    #[test]
    fn only_the_exact_build_for_this_node_is_accepted() {
        let local = "--features  llama/mac-cpu-only,livekit-webrtc";
        assert_eq!(
            manifest_verdict(&manifest(), TIP, "macos-x86_64", local),
            Ok(()),
            "whitespace is not a difference"
        );
        assert!(
            manifest_verdict(&manifest(), &TIP.replace('5', "6"), "macos-x86_64", local).is_err(),
            "another commit"
        );
        assert!(
            manifest_verdict(&manifest(), TIP, "macos-arm64", local).is_err(),
            "another platform"
        );
        assert!(
            manifest_verdict(
                &manifest(),
                TIP,
                "macos-x86_64",
                "--features metal,accelerate"
            )
            .is_err(),
            "another feature set"
        );
        let mut short = manifest();
        short.git_sha = TIP[..9].into();
        assert!(
            manifest_verdict(&short, TIP, "macos-x86_64", local).is_err(),
            "a short sha is not provenance"
        );
        let mut partial = manifest();
        partial.bins.retain(|b| b != "forge-custodian");
        assert!(
            manifest_verdict(&partial, TIP, "macos-x86_64", local).is_err(),
            "a sibling bin is missing"
        );
        // Regression for card68a33e89: a fresh user's missing engine must never
        // select a developer-toolchain fallback or a different fork revision.
        let mut fresh = manifest();
        fresh.platform = "windows-x86_64".into();
        assert!(fresh_engine_verdict(&fresh, TIP).is_err());
        fresh.engine = Some(EngineArtifact {
            source_revision: TIP.into(),
            backend: "cuda".into(),
            relative_path: "engine".into(),
        });
        assert_eq!(fresh_engine_verdict(&fresh, TIP), Ok(()));
        assert!(fresh_engine_verdict(&fresh, &TIP.replace('5', "6")).is_err());
        fresh.engine.as_mut().unwrap().relative_path = "../engine".into();
        assert!(fresh_engine_verdict(&fresh, TIP).is_err());
        assert!(archive_members_verdict(
            "continuum-core-windows-x86_64/continuum.exe\n",
            "-rw file\n",
            "windows-x86_64"
        )
        .is_ok());
        let gnu = "drwxr-xr-x user/group 0 2026-10-16 10:00 root/\n-rw-r--r-- user/group 123 2026-10-16 10:00 root/a name\n";
        let bsd = "drwxr-xr-x 0 user group 0 Oct 16 10:00 root/\n-rw-r--r-- 0 user group 456 Oct 16 10:00 root/b\n";
        assert_eq!(archive_expanded_bytes(gnu).unwrap(), 123);
        assert_eq!(archive_expanded_bytes(bsd).unwrap(), 456);
        for bad in ["", "-rw file", "lrwxr-xr-x user/group 1 2026-10-16 10:00 link", "-rw-r--r-- user/group nope 2026-10-16 10:00 file"] {
            assert!(archive_expanded_bytes(bad).is_err());
        }
        assert!(archive_expanded_bytes(&format!("-rw-r--r-- user/group {} 2026-10-16 10:00 a\n-rw-r--r-- user/group 1 2026-10-16 10:00 b\n", u64::MAX)).is_err());
        for names in [
            "../escape",
            "continuum-core-windows-x86_64/../escape",
            "continuum-core-windows-x86_64/cli\ncontinuum-core-windows-x86_64/CLI",
        ] {
            assert!(archive_members_verdict(names, "-rw file\n", "windows-x86_64").is_err());
        }
        assert!(archive_members_verdict(
            "continuum-core-windows-x86_64/cli",
            "lrw link",
            "windows-x86_64"
        )
        .is_err());
    }

    // what this catches: a docs-only tip forcing every node to rebuild (and CI to build) a
    // binary whose inputs did not change; and the reverse, a dependency bump in the ROOT
    // Cargo.lock being treated as not-a-build (#4691 first shipped with that hole).
    #[test]
    fn only_a_change_to_a_build_input_is_a_deploy() {
        assert!(!touches_build_inputs([
            "README.md",
            "docs/planning/VOICE-ENGINE-PLAN.md"
        ]));
        assert!(
            !touches_build_inputs(["core-notes.md", "coreutils/x"]),
            "a prefix is a directory, not a substring"
        );
        assert!(touches_build_inputs([
            "docs/x.md",
            "core/continuum-core/src/lib.rs"
        ]));
        assert!(touches_build_inputs(["Cargo.lock"]));
        assert!(touches_build_inputs(["tools/scripts/lib/core-features.sh"]));
    }

    // what this catches: the Rust list and CI's trigger drifting apart, so a tip the node
    // treats as a deploy has no artifact (it waits out the budget, then compiles anyway).
    #[test]
    fn ci_builds_on_every_build_input() {
        let workflow = include_str!("../../../../.github/workflows/core-binaries.yml");
        let push = workflow
            .split("  push:")
            .nth(1)
            .expect("core-binaries.yml has a push trigger");
        for input in BUILD_INPUTS {
            let pattern = match input.strip_suffix('/') {
                Some(dir) => format!("- '{dir}/**'"),
                None => format!("- '{input}'"),
            };
            assert!(
                push.contains(&pattern),
                "core-binaries.yml push.paths lacks {pattern}"
            );
        }
    }

    // what this catches (f4571736): user nodes must never fall back to compiling when
    // CI is late or does not support their platform. Explicit developer builds are separate.
    #[test]
    fn a_missing_artifact_waits_only_while_ci_can_still_deliver() {
        assert!(matches!(
            when_artifact_missing(Some("macos-x86_64"), 20 * 60),
            MissingArtifact::Wait(_)
        ));
        assert!(matches!(
            when_artifact_missing(Some("macos-x86_64"), CI_PUBLISH_BUDGET_SECS),
            MissingArtifact::Refuse(_)
        ));
        assert!(matches!(
            when_artifact_missing(None, 0),
            MissingArtifact::Refuse(_)
        ));
        assert_eq!(platform_key("macos", "x86_64"), Some("macos-x86_64"));
        assert_eq!(platform_key("windows", "x86_64"), Some("windows-x86_64"));
        assert_eq!(platform_key("linux", "x86_64"), None);
        assert_eq!(release_tag(TIP).as_deref(), Some("canary-54cbe937f012"));
        assert_eq!(
            release_tag("54cbe937f"),
            None,
            "a short sha cannot name a release"
        );
    }

    // what this catches: the build key must be computed over exactly the inputs CI builds
    // on (the workflow's paths are pinned to BUILD_INPUTS by the drift test above), with
    // the tip before `--` so git reads every input as a path.
    #[test]
    fn the_build_key_query_covers_every_build_input() {
        let args = build_key_log_args("4e3bc6477");
        let sep = args.iter().position(|a| a == "--").expect("a -- separator");
        assert_eq!(&args[..sep], ["log", "-1", "--format=%H", "4e3bc6477"]);
        assert_eq!(&args[sep + 1..], BUILD_INPUTS.map(String::from));
    }

    // what this catches (IntelMac, 2026-10-06): a node that waits forever on the newest tip
    // while an older build is published. Core 5b496e307 ran from 12:30Z while 8de501e35's
    // core had been out since 15:18Z: 5dbfadbca was building and e428cbd50 queued, and the
    // consumer only ever waited on the tip. The newest PUBLISHED candidate is the deploy.
    #[tokio::test]
    async fn the_newest_published_build_is_deployed_not_only_the_tip() {
        async fn newest_published(candidates: &[String], published: &[bool], verdicts: &[Option<crate::deploy_tracker::Checks>]) -> Option<String> {
            struct Source<'a> {
                keys: &'a [String],
                published: &'a [bool],
                verdicts: &'a [Option<crate::deploy_tracker::Checks>],
                fetched: std::sync::Mutex<Vec<String>>,
            }
            #[async_trait::async_trait]
            impl PublishedCoreSource for Source<'_> {
                type Artifact = String;
                async fn published(&self, key: &str) -> Result<bool, String> {
                    Ok(self.published[self.keys.iter().position(|k| k == key).unwrap()])
                }
                async fn checks(&self, key: &str) -> Result<crate::deploy_tracker::Checks, String> {
                    let verdict = self.verdicts[self.keys.iter().position(|k| k == key).unwrap()];
                    // None exercises a failed read; Some(Unknown) tests an explicit unknown verdict.
                    verdict.ok_or_else(|| "CI source unavailable".into())
                }
                async fn fetch(&self, key: &str) -> Result<Option<String>, PublishedCoreError> {
                    self.fetched.lock().unwrap().push(key.to_string());
                    Ok(Some(key.to_string()))
                }
            }
            let source = Source { keys: candidates, published, verdicts, fetched: Default::default() };
            let selected = select_published_core(candidates, &source).await;
            let key = selected.map(|(key, artifact)| { assert_eq!(artifact.unwrap(), Some(key.clone())); key });
            assert_eq!(*source.fetched.lock().unwrap(), key.iter().cloned().collect::<Vec<_>>(),
                "only the authorized revision may reach artifact fetch");
            key
        }
        let candidates: Vec<String> = ["e428cbd50", "5dbfadbca", "8de501e35"]
            .map(String::from)
            .to_vec();
        use crate::deploy_tracker::Checks;
        let green = [Some(Checks::Green); 3];
        assert_eq!(
            newest_published(&candidates, &[false, false, true], &green).await.as_deref(),
            Some("8de501e35")
        );
        assert_eq!(
            newest_published(&candidates, &[false, true, true], &green).await.as_deref(),
            Some("5dbfadbca"),
            "the newest out wins"
        );
        assert_eq!(
            newest_published(&candidates, &[true, false, true], &green).await.as_deref(),
            Some("e428cbd50"),
            "the tip, when it is out"
        );
        assert_eq!(
            newest_published(&candidates, &[false, false, false], &green).await.as_deref(),
            None,
            "none out: wait"
        );
        for refused in [Some(Checks::Red), Some(Checks::Pending), Some(Checks::Unknown), None] {
            assert_eq!(newest_published(&candidates, &[false, true, true], &[Some(Checks::Green), refused, Some(Checks::Green)]).await.as_deref(),
                Some("8de501e35"), "a green request cannot authorize a non-green published ancestor");
            assert_eq!(newest_published(&candidates, &[false, true, false], &[Some(Checks::Green), refused, Some(Checks::Unknown)]).await.as_deref(),
                None, "without a published green candidate the running core stands");
        }
        // A published ancestor under resource pressure remains published and authorized,
        // but may not redownload until the same measured condition clears.
        struct DeferredSource { pending: Option<(String, ResourceDeferral)>, fetches: std::sync::atomic::AtomicUsize }
        #[async_trait::async_trait]
        impl PublishedCoreSource for DeferredSource {
            type Artifact = String;
            fn deferred(&self, key: &str) -> Option<ResourceDeferral> {
                self.pending.as_ref().filter(|(waiting, _)| waiting == key).map(|(_, d)| d.clone())
            }
            async fn published(&self, _: &str) -> Result<bool, String> { Ok(true) }
            async fn checks(&self, _: &str) -> Result<Checks, String> { Ok(Checks::Green) }
            async fn fetch(&self, key: &str) -> Result<Option<String>, PublishedCoreError> {
                self.fetches.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(Some(key.into()))
            }
        }
        let condition = ResourceDeferral { condition: ResourceCondition::VolumeCapacity, path: "/artifact-cache".into(), projected_bytes: 579, reason: "measured capacity refusal".into() };
        let mut source = DeferredSource { pending: Some(("older".into(), condition.clone())), fetches: Default::default() };
        for _ in 0..3 {
            assert!(matches!(recheck_resource_deferral(&mut source.pending, Err(PublishedCoreError::ResourceDeferred(condition.clone()))), Some(MissingArtifact::AdmissionWait(_))));
            assert!(matches!(select_published_core(&["older".into()], &source).await, Some((_, Err(PublishedCoreError::ResourceDeferred(_))))));
        }
        assert_eq!(source.fetches.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(select_published_core(&["newer".into(), "older".into()], &source).await.unwrap().1.is_ok(), "different published candidate remains eligible");
        assert!(recheck_resource_deferral(&mut source.pending, Ok(())).is_none());
        assert!(source.pending.is_none());
        assert!(select_published_core(&["older".into()], &source).await.unwrap().1.is_ok());
        assert_eq!(source.fetches.load(std::sync::atomic::Ordering::SeqCst), 2);
        for kind in [ResourceCondition::UnmeasuredFilesystem, ResourceCondition::OperationalUncertainty] {
            let mut blocked = condition.clone(); blocked.condition = kind;
            source.pending = Some(("older".into(), blocked.clone()));
            assert!(matches!(recheck_resource_deferral(&mut source.pending, Ok(())), Some(MissingArtifact::AdmissionWait(_))));
            assert_eq!(source.pending.as_ref().unwrap().1, blocked);
            assert!(matches!(select_published_core(&["older".into()], &source).await, Some((_, Err(PublishedCoreError::ResourceDeferred(_))))));
        }
        assert_eq!(source.fetches.load(std::sync::atomic::Ordering::SeqCst), 2, "operational uncertainty cannot refill by automatic fetch");
        let args = candidate_keys_log_args("5b496e307", "e428cbd50");
        assert!(
            args.contains(&"5b496e307..e428cbd50".to_string()),
            "newer than the running core only"
        );
        assert!(args.contains(&format!("-{DEPLOY_CANDIDATE_LIMIT}")));
        assert!(
            args.iter().skip_while(|a| *a != "--").any(|a| a == "core/"),
            "build inputs only"
        );
    }
