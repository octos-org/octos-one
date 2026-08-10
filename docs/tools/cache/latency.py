import json, os, time, urllib.request, statistics
from cache_experiment import baseline
key = os.environ["OPENAI_API_KEY"]; base = baseline()

def stream_call(text, n_out):
    """Returns (ttft, total, cached_tokens) using a streaming response."""
    payload = {"model": "gpt-4o-mini",
               "messages": [{"role": "user", "content": text}],
               "max_completion_tokens": n_out, "stream": True,
               "stream_options": {"include_usage": True}}
    req = urllib.request.Request("https://api.openai.com/v1/chat/completions",
        data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"})
    t0 = time.time(); ttft = None; cached = 0
    with urllib.request.urlopen(req, timeout=120) as r:
        for raw in r:
            line = raw.decode().strip()
            if not line.startswith("data: "): continue
            body = line[6:]
            if body == "[DONE]": break
            d = json.loads(body)
            if d.get("choices") and d["choices"][0].get("delta", {}).get("content") and ttft is None:
                ttft = time.time() - t0
            if d.get("usage"):
                cached = d["usage"].get("prompt_tokens_details", {}).get("cached_tokens", 0)
    return ttft, time.time() - t0, cached

def bench(label, make_text, n_out, reps=4):
    ttfts, totals, cach = [], [], []
    for i in range(reps):
        a, b, c = stream_call(make_text(i), n_out)
        if a: ttfts.append(a); totals.append(b); cach.append(c)
        time.sleep(1)
    print(f"  {label:<28} TTFT {statistics.median(ttfts):5.2f}s   total {statistics.median(totals):5.2f}s"
          f"   cached {statistics.median(cach):>6.0f}")

print("SHORT output (16 tokens) — prefill-dominated, like a ledger edit")
bench("cold (unique prefix)", lambda i: f"# run-{i}-{time.time()}\n" + base + "\n\nGo.", 16)
bench("warm (cached prefix)", lambda i: base + "\n\nGo.", 16)
print("\nLONG output (400 tokens) — decode-dominated")
bench("cold (unique prefix)", lambda i: f"# runL-{i}-{time.time()}\n" + base + "\n\nWrite a long story.", 400)
bench("warm (cached prefix)", lambda i: base + "\n\nWrite a long story.", 400)
