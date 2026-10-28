#!/bin/bash
cd /Users/joel/.airc/worktrees/95bc81f4
cargo check -p continuum-dreaming 2>&1 | head -100
