#!/bin/zsh
# TIER BATTERY — the standing model gauntlet that seated Ornith-1.5-35B on 2026-08-22
# (5-6.7x prefill vs Qwen3-30B on this M5, 8/8 native tool calls @1.2s, vision PASS).
# Promoted from session scratchpad so the gauntlet survives the operator (a model
# audition must never be hand-ops re-invented per session).
#
# TRAJECTORY (Joel, 2026-08-22): the model catalog and tables like it become entity
# data configurable by commands — when that lands, this script's verdict feeds a
# `model/audition` command writing a candidate row, and the file is retired.
# TIER BATTERY — the 27-35B tier seat is decided by THIS, on OUR flags, receipts only.
# Usage: tier_battery.sh <name> <gguf> <mmproj|-> <extra-args...>
# Requires: NO other model resident (quiesced window). Writes receipts to
# ~/.continuum/bench-receipts/tier-battery-<name>.md
set -e
NAME=$1; GGUF=$2; MMPROJ=$3; shift 3 || true
PORT=58910
OUT=~/.continuum/bench-receipts/tier-battery-$NAME.md
mkdir -p ~/.continuum/bench-receipts
BENCH=~/.continuum/cache/llama-server-build/bin/llama-bench
SRV=~/.continuum/bin/llama-server

echo "# Tier battery: $NAME — $(date '+%Y-%m-%d %H:%M')" > $OUT
echo "gguf: $GGUF" >> $OUT

## 1. Raw engine matrix (solo, no server): prefill+decode at depth 0 and 16k
echo "\n## 1. llama-bench (fa on, q8 KV, ub 2048)" >> $OUT
$BENCH -m $GGUF -fa 1 -ctk q8_0 -ctv q8_0 -ub 2048 -p 4096 -n 128 -d 0,16384 -r 2 -o md 2>/dev/null >> $OUT

## 2. Serving battery: 4-slot server, OUR production flag shape
ARGS=(-m $GGUF --host 127.0.0.1 --port $PORT -c 131072 --parallel 4
      --flash-attn on --cache-type-k q8_0 --cache-type-v q8_0
      --cache-reuse 256 --ubatch-size 2048 --no-context-shift --jinja)
[ "$MMPROJ" != "-" ] && ARGS+=(--mmproj $MMPROJ)
$SRV $ARGS "$@" > /tmp/tier-$NAME-server.log 2>&1 &
SPID=$!
trap "kill $SPID 2>/dev/null" EXIT
until curl -s http://127.0.0.1:$PORT/health 2>/dev/null | grep -q ok; do sleep 3; done

echo "\n## 2. Template caps" >> $OUT
curl -s http://127.0.0.1:$PORT/props | python3 -c "import sys,json; print(json.load(sys.stdin).get('chat_template_caps'))" >> $OUT

## 3. Tool battery: 8 tool-shaped prompts, our sampling; score native-call rate
echo "\n## 3. Tool fidelity (our sampling: temp .7, no repeat-penalty hack)" >> $OUT
python3 - "$PORT" >> $OUT <<'PY'
import sys,json,urllib.request,time
port=sys.argv[1]
TOOLS=[{"type":"function","function":{"name":n,"description":d,"parameters":{"type":"object","properties":p,"required":list(p)}}}
 for n,d,p in [
  ("read_file","Read a file",{"file_path":{"type":"string"}}),
  ("write_file","Write a file",{"file_path":{"type":"string"},"content":{"type":"string"}}),
  ("bash","Run a shell command",{"cmd":{"type":"string"}}),
  ("list_files","List a directory",{"path":{"type":"string"}}),
]]
PROMPTS=[
 "Read the file src/main.rs.",
 "List the tests/ directory, then read tests/test_api.py.",
 "Run the test suite with pytest -x.",
 "Create hello.py containing print('hi') then run it.",
 "What files are in the repository root?",
 "Check git status and show me the diff.",
 "Read config.toml and fix the port to 8080.",
 "Find every TODO in src/ using grep.",
]
ok=0; multi=0; lat=[]
for p in PROMPTS:
    body=json.dumps({"model":"m","max_tokens":1500,"temperature":0.7,
      "messages":[{"role":"system","content":"You are a coding agent. Use tools to act; do not describe."},
                  {"role":"user","content":p}],"tools":TOOLS}).encode()
    t0=time.time()
    try:
        r=urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",body,{"Content-Type":"application/json"}),timeout=300)
        d=json.load(r); lat.append(time.time()-t0)
        tc=(d["choices"][0]["message"].get("tool_calls") or [])
        names=[c["function"]["name"] for c in tc]
        valid=all(json.loads(c["function"]["arguments"]) is not None for c in tc) if tc else False
        if tc and valid: ok+=1
        if len(tc)>1: multi+=1
        print(f"- {p[:44]:46s} calls={names} args_valid={valid} {lat[-1]:.1f}s")
    except Exception as e:
        print(f"- {p[:44]:46s} ERROR {e}")
print(f"\nNATIVE-CALL RATE: {ok}/{len(PROMPTS)}  multi-call turns: {multi}  median latency: {sorted(lat)[len(lat)//2]:.1f}s" if lat else "no latencies")
PY

## 4. Vision smoke (only if mmproj)
if [ "$MMPROJ" != "-" ]; then
echo "\n## 4. Vision smoke" >> $OUT
python3 - "$PORT" >> $OUT <<'PY'
import sys,json,urllib.request,base64,struct,zlib
port=sys.argv[1]
def png(w,h,rgb):
    raw=b''.join(b'\x00'+bytes(rgb)*w for _ in range(h))
    def ch(t,d): c=t+d; return struct.pack('>I',len(d))+c+struct.pack('>I',zlib.crc32(c))
    return b'\x89PNG\r\n\x1a\n'+ch(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+ch(b'IDAT',zlib.compress(raw))+ch(b'IEND',b'')
img=base64.b64encode(png(64,64,(255,0,0))).decode()
body=json.dumps({"model":"m","max_tokens":100,"messages":[{"role":"user","content":[
  {"type":"text","text":"What color is this image? One word."},
  {"type":"image_url","image_url":{"url":f"data:image/png;base64,{img}"}}]}]}).encode()
try:
    r=urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",body,{"Content-Type":"application/json"}),timeout=180)
    d=json.load(r); ans=d["choices"][0]["message"].get("content","")
    print(f"red-square test → {ans[:80]!r}  PASS={'red' in ans.lower()}")
except Exception as e: print("vision ERROR:",e)
PY
fi

## 5. 4-slot concurrent throughput
echo "\n## 5. Concurrent (4 parallel 2k-token prompts)" >> $OUT
python3 - "$PORT" >> $OUT <<'PY'
import sys,json,urllib.request,time,threading
port=sys.argv[1]; res=[]
def one(i):
    filler=" ".join(f"w{i}{j}" for j in range(1500))
    body=json.dumps({"model":"m","max_tokens":64,"messages":[{"role":"user","content":filler+" Reply OK."}]}).encode()
    t0=time.time()
    try:
        urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",body,{"Content-Type":"application/json"}),timeout=300).read()
        res.append(time.time()-t0)
    except Exception as e: res.append(-1)
ts=[threading.Thread(target=one,args=(i,)) for i in range(4)]
t0=time.time()
[t.start() for t in ts]; [t.join() for t in ts]
print(f"4x ~2k prompts wall-clock: {time.time()-t0:.1f}s  per-req: {[f'{r:.1f}' for r in res]}")
PY

kill $SPID 2>/dev/null; trap - EXIT
echo "\nBattery complete → $OUT"
cat $OUT | tail -5
