#!/usr/bin/env python3
"""
toolcall_shim.py — a fairness fixture for benchmarking cloud-built harnesses on LOCAL models.

Local models (Qwen via llama-server) emit tool calls under `tool_choice:auto` as NARRATED text
(`<tools>{...}</tools>`, `<tool_call>{...}</tool_call>`, or a ```json block), NOT a native
`tool_calls` field — so opencode/Aider drop them and can't act. Our OWN system parses those
formats (json_in_prompt_tools.rs). This shim gives the OPPONENT the same recovery, so its score
measures its LOOP, not a tool-format gap we happen to have closed and it hasn't. Give the
opponent its best shot — then whatever delta remains is honestly our system's, not our parser's.

It is a transparent /v1 proxy: forward each request to the upstream llama-server, and if the
assistant message narrated a tool call in content, lift it into a native `tool_calls` field.
Streaming is coerced off (stream=false) so the parse is a single clean pass. Pure stdlib —
an EDGE benchmark tool, never in the operational path.

  python3 toolcall_shim.py --listen 8094 --upstream http://127.0.0.1:8093
"""
import argparse
import json
import re
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

UPSTREAM = "http://127.0.0.1:8093"

# Narrated tool-call shapes local models emit under auto. Each yields {name, arguments-json}.
_TAG = re.compile(r"<(tool_call|tools)>\s*(\{.*?\})\s*</\1>", re.S)
_FENCE = re.compile(r"```(?:json)?\s*(\{.*?\})\s*```", re.S)


def _extract_calls(content: str):
    """Find narrated tool calls in `content`; return OpenAI-format tool_calls (or [])."""
    found = []
    for m in list(_TAG.finditer(content)) + list(_FENCE.finditer(content)):
        blob = m.group(2) if m.re is _TAG else m.group(1)
        try:
            obj = json.loads(blob)
        except json.JSONDecodeError:
            continue
        name = obj.get("name")
        args = obj.get("arguments", obj.get("parameters"))
        if not name or args is None:
            continue
        found.append({
            "id": f"shim_{len(found)}_{abs(hash(blob)) % (10**8)}",
            "type": "function",
            "function": {"name": name, "arguments": args if isinstance(args, str) else json.dumps(args)},
        })
    return found


def _recover(resp: dict) -> dict:
    """If the assistant narrated a call but has no native tool_calls, lift it in."""
    for choice in resp.get("choices", []):
        msg = choice.get("message") or {}
        if msg.get("tool_calls"):
            continue
        content = msg.get("content") or ""
        calls = _extract_calls(content)
        if calls:
            msg["tool_calls"] = calls
            msg["content"] = ""  # the call replaces the narration
            choice["finish_reason"] = "tool_calls"
    return resp


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _proxy(self, method):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0))) if method == "POST" else None
        # coerce streaming off UPSTREAM so we get one complete JSON to parse — but remember
        # whether the CLIENT wanted a stream, so we can re-emit SSE for it below.
        client_wants_stream = False
        if body:
            try:
                j = json.loads(body)
                client_wants_stream = bool(j.get("stream"))
                if client_wants_stream:
                    j["stream"] = False
                    body = json.dumps(j).encode()
            except json.JSONDecodeError:
                pass
        req = urllib.request.Request(UPSTREAM + self.path, data=body, method=method)
        req.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(req, timeout=600) as up:
                raw = up.read()
        except Exception as e:  # surface upstream failure, don't hang the harness
            self.send_response(502); self.end_headers(); self.wfile.write(str(e).encode()); return
        # recover narrated tool calls on chat completions
        resp = None
        if "chat/completions" in self.path:
            try:
                resp = _recover(json.loads(raw))
                raw = json.dumps(resp).encode()
            except json.JSONDecodeError:
                pass
        # Client asked for a stream: re-emit the (recovered) response as one SSE burst so the
        # ai-sdk streaming parser accepts it (with native tool_calls now present).
        if client_wants_stream and resp is not None:
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            cid = resp.get("id", "shimchunk")
            for choice in resp.get("choices", []):
                msg = choice.get("message") or {}
                delta = {"role": "assistant"}
                if msg.get("content"):
                    delta["content"] = msg["content"]
                if msg.get("tool_calls"):
                    delta["tool_calls"] = [
                        {"index": i, **tc} for i, tc in enumerate(msg["tool_calls"])
                    ]
                chunk = {"id": cid, "object": "chat.completion.chunk",
                         "choices": [{"index": choice.get("index", 0), "delta": delta,
                                      "finish_reason": None}]}
                self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
                fin = {"id": cid, "object": "chat.completion.chunk",
                       "choices": [{"index": choice.get("index", 0), "delta": {},
                                    "finish_reason": choice.get("finish_reason", "stop")}]}
                self.wfile.write(f"data: {json.dumps(fin)}\n\n".encode())
            self.wfile.write(b"data: [DONE]\n\n")
            return
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_POST(self):
        self._proxy("POST")

    def do_GET(self):
        self._proxy("GET")


def main():
    global UPSTREAM
    ap = argparse.ArgumentParser()
    ap.add_argument("--listen", type=int, default=8094)
    ap.add_argument("--upstream", default=UPSTREAM)
    args = ap.parse_args()
    UPSTREAM = args.upstream
    print(f"toolcall shim: :{args.listen} → {UPSTREAM} (recovering narrated tool calls)", flush=True)
    ThreadingHTTPServer(("127.0.0.1", args.listen), Handler).serve_forever()


if __name__ == "__main__":
    main()
