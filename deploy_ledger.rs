--- a/core/continuum-core/src/bin/continuum/deploy_consume.rs
+++ b/core/continuum-core/src/bin/continuum/deploy_consume.rs
@@ -330,7 +330,8 @@ impl DeployConsumer {
                 // check the tree out under this one and reboot alongside it. `reboot` re-takes
                 // the same claim for this pid, and its guard releases it at the handoff.
                 let _claim = DeployClaimGuard::take(&tip);
-                git_in(&repo, &["fetch", "--quiet", "origin"])?;
+                git_in(
+                    &repo,
+                    &["fetch", "-c", "submodule.recurse=false", "--quiet", "origin"],
+                )?;
                 git_in(&repo, &["checkout", "--quiet", "--detach", &tip])?;
