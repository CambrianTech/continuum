SINCE=$(( $(date -d 'today 00:00' +%s) * 1000 ))
echo "since_ms=$SINCE"
which continuum || ls ~/.continuum/bin 2>/dev/null; echo ---
continuum debug/probes/query --class airc.command.completed --contains work/submission --since-ms $SINCE --limit 20
