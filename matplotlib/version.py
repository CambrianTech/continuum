set +e
B=/c/Users/joelt/.continuum/benchmarks/swe
echo ===ENVS===; ls "$B/envs" 2>/dev/null | head -40
echo ===WORK===; ls "$B/work" 2>/dev/null | head -40
echo ===ROW===
python -c "import json,io;d=json.load(io.open(r'C:/Users/joelt/.continuum/benchmarks/swe/princeton-nlp__SWE-bench_Verified.json',encoding='utf-8'));[print({k:r.get(k) for k in ('instance_id','base_commit','environment_setup_commit','repo')}) for r in d if '20488' in str(r.get('instance_id',''))]"
D=swe/matplotlib__matplotlib-20488.cloning-30052-41
cd "$D" || exit 1
echo ===COMMIT===
git cat-file -t b7ce415c15eb39b026a097a2865da73fbcf15c9c
if git cat-file -e b7ce415c15eb39b026a097a2865da73fbcf15c9c; then
  git checkout --detach -q b7ce415c15eb39b026a097a2865da73fbcf15c9c && echo CHECKED-OUT
fi
git log --oneline -1
echo ===TEST===
grep -n "def test_huge_range_log" lib/matplotlib/tests/test_image.py || echo NOT-IN-CHECKOUT
cd .. && [ -d swe/matplotlib__matplotlib-20488 ] || mv "$D" swe/matplotlib__matplotlib-20488; ls swe/
