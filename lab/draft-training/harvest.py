#!/usr/bin/env python3
"""Phase-1 distillation harvest for the Qwen3.8 card-draft project.

Generates temp-0 target outputs on the REAL all-apps prompt in three families:
  pick    - single-app sweep (the model picks the app, as production does)
  compose - multi-domain composed pages (PICKING paragraph swapped for a
            composition permission) -- the priority training family
  general - prose/summary traffic so the draft keeps non-card acceptance

Resilient: resumes from out.jsonl, retries with backoff, restarts the serving
container after repeated connection failures. Designed to run inside tmux.
"""
import json, urllib.request, time, threading, queue, subprocess, os, sys

BASE_DIR = "/home/ubuntu/qwen38-h200"
OUT = os.path.join(BASE_DIR, "harvest", "out.jsonl")
URL = "http://127.0.0.1:30878/v1/chat/completions"
MODEL = "Qwen3.8-27B-FP8-DFlash"
WORKERS = 4
os.makedirs(os.path.dirname(OUT), exist_ok=True)

base = json.load(open(os.path.join(BASE_DIR, "warm_request.json")))
base.pop("tools", None); base.pop("tool_choice", None)
base["stream"] = False; base["temperature"] = 0; base["max_tokens"] = 4096
base["model"] = MODEL

PICK_SENTINEL = "PICK the ONE app that answers the user request, then write THAT app's card"
EMIT_SENTINEL = "the complete card \\\nfor the ONE app you picked"
_user = next(m["content"] for m in base["messages"] if m.get("role") == "user" and isinstance(m.get("content"), str) and "PICK" in m.get("content", ""))
assert PICK_SENTINEL in _user, "prompt shape changed; update sentinels"

COMPOSE_TEXT = ("COMPOSE one card that answers the user request, drawing sections from AS MANY "
    "of the APP sections below as the request spans (weather panes, event/activity lists, "
    "nav routes, video rows, stock tiles, news feeds...). Declare each section's sources per "
    "its own app spec, share common state (like the place) across sections, and keep every "
    "L0 rule. If the request spans one domain only, a single-app card is correct")

def make_request(query, mode):
    d = json.loads(json.dumps(base))
    for m in d["messages"]:
        c = m.get("content")
        if isinstance(c, str) and "weather in Berlin" in c:
            c = c.replace("weather in Berlin", query)
            if mode == "compose":
                c = c.replace(PICK_SENTINEL, COMPOSE_TEXT)
                c = c.replace("for the ONE app you picked", "for the composition")
            m["content"] = c
    return d

def make_general(query):
    d = json.loads(json.dumps(base))
    d["messages"] = [{"role": "user", "content": query}]
    d["max_tokens"] = 1500
    return d

# ---- job matrix ------------------------------------------------------------
CITIES = ("Tokyo London Paris Berlin Madrid Rome Vienna Prague Lisbon Athens Cairo Oslo "
    "Helsinki Warsaw Dublin Zurich Porto Naples Lyon Turin Seville Genoa Nice Basel Ghent "
    "Leeds Bergen Aarhus Graz Brno Kyoto Osaka Seoul Busan Taipei Singapore Bangkok Hanoi "
    "Mumbai Delhi Nairobi Lagos Casablanca Istanbul Dubai Doha Sydney Melbourne Auckland "
    "Toronto Vancouver Montreal Chicago Boston Seattle Denver Austin Miami Havana Lima "
    "Bogota Santiago Quito Reykjavik Tallinn Riga Vilnius Krakow Zagreb Belgrade Sofia").split()
TICKERS = ("NVDA AAPL MSFT GOOG AMZN META TSLA AMD INTC AVGO ORCL CRM ADBE NFLX QCOM TXN "
    "MU AMAT ASML TSM BABA JD PDD NIO XPEV LI UBER ABNB SHOP SQ COIN PLTR SNOW DDOG NET").split()
NEWS = ["AI", "climate", "space exploration", "semiconductors", "electric vehicles", "quantum computing",
    "open source", "robotics", "biotech", "cybersecurity", "energy", "startups", "world", "business",
    "science", "the olympics", "elections", "markets", "chips", "programming languages"]
NAVS = ["the airport", "the nearest coffee shop", "downtown", "the train station", "nvidia headquarters",
    "apple park", "the golden gate bridge", "union square", "the nearest hospital", "the beach",
    "stanford university", "the ferry building", "city hall", "the nearest gas station", "chinatown"]
YT = ["jazz music", "lofi beats", "cooking pasta", "tokyo travel", "rust programming", "machine learning",
    "surfing", "chess openings", "woodworking", "night sky photography", "marathon training", "espresso"]
THEMES = ["dark", "light", "minimal", "glass", "vibrant", "photo"]

jobs = []
def add(family, mode, query):
    jobs.append({"id": f"{family}-{len(jobs):05d}", "family": family, "mode": mode, "query": query})

# pick family (~2400)
for c in CITIES:
    add("weather", "pick", f"weather in {c}")
    add("weather", "pick", f"{c} forecast this week")
for c in CITIES[:24]:
    for t in THEMES:
        add("weather-theme", "pick", f"{t} weather {c}")
for t in TICKERS:
    add("stock", "pick", f"{t} stock price")
    add("stock", "pick", f"how is {t} doing today")
for q in ["top movers today", "best performing stocks", "market gainers", "biggest losers today"]:
    add("stock", "pick", q)
for n in NEWS:
    add("news", "pick", f"latest {n} news")
    add("news", "pick", f"top {n} headlines")
for n in NAVS:
    add("nav", "pick", f"navigate to {n}")
    add("nav", "pick", f"directions to {n}")
for y in YT:
    add("youtube", "pick", f"{y} videos")
    add("youtube", "pick", f"watch {y}")
for q in ["km to miles", "20C in fahrenheit", "kg to lbs", "100 mph in kmh", "5 miles in km",
          "30 celsius to fahrenheit", "10 stone in kg", "2 liters in gallons"]:
    add("convert", "pick", q)
for q in ["earthquakes today", "recent quakes", "any earthquakes near japan", "seismic activity this week"]:
    add("quake", "pick", q)
for a, b in [("china", "india"), ("japan", "korea"), ("usa", "china"), ("germany", "france"),
             ("brazil", "mexico"), ("india", "vietnam"), ("nigeria", "egypt"), ("spain", "italy")]:
    add("chart", "pick", f"{a} gdp growth vs {b}")
for c in CITIES[:20]:
    add("activity", "pick", f"things to do in {c}")
    add("weather-activity", "pick", f"what can I do in {c} today")
add("city-picks", "pick", "where should I go across my saved cities")
add("city-picks", "pick", "which of my cities is nicest right now")

# compose family (~1600) -- the priority
TRAVEL_COMBOS = [
    "a travel page for {c}: current weather, top things to do, and how to get around",
    "plan a trip to {c} — weather this week, local events, and videos about the city",
    "a {c} city guide card: weather, activities, and directions from the airport",
    "one page for my {c} visit: forecast, what to do today, and a news section about {c}",
    "compose a {c} dashboard: weather, nearby activities, and travel videos",
]
for c in CITIES[:56]:
    for t in TRAVEL_COMBOS:
        add("travel", "compose", t.format(c=c))
DASHBOARDS = [
    "my morning dashboard: weather here, top stocks, and top headlines",
    "a market morning card: NVDA and AAPL tiles plus business news",
    "commute card: weather now plus directions to the office",
    "an evening card: weather tonight and jazz videos",
    "a weekend planner: weather saturday, things to do, and event videos",
    "tech pulse: chip stocks and semiconductor news in one card",
    "a storm tracker: weather, earthquake feed, and emergency news",
    "a foodie card for {c}: nearby restaurants and food videos",
    "a runner's card: weather, air quality, and a route to the park",
    "compare {a} and {b} gdp plus their market news",
]
for i, d in enumerate(DASHBOARDS):
    for c in CITIES[:30]:
        q = d.format(c=c, a="china", b="india")
        add("dashboard", "compose", q)

# general family (~10%)
GENERAL = [
    "Summarize the tradeoffs between speculative decoding with a draft model versus n-gram lookahead.",
    "Write a short paragraph explaining what a KV cache is.",
    "Explain the difference between prefill and decode in LLM serving.",
    "Write a haiku about GPUs.",
    "List five considerations when quantizing a model to FP8.",
    "Explain what a systemd unit file does.",
    "Describe how a suffix automaton works in two paragraphs.",
    "Write a friendly reminder email about a team meeting tomorrow at 10am.",
    "Explain CUDA graphs to a new engineer.",
    "Summarize why byte-stable prompts matter for prefix caching.",
]
for i in range(40):
    for g in GENERAL:
        add("general", "general", g + f" (variant {i})")

print(f"job matrix: {len(jobs)} jobs "
      f"(pick={sum(1 for j in jobs if j['mode']=='pick')}, "
      f"compose={sum(1 for j in jobs if j['mode']=='compose')}, "
      f"general={sum(1 for j in jobs if j['mode']=='general')})", flush=True)

# ---- resume ----------------------------------------------------------------
done = set()
if os.path.exists(OUT):
    for line in open(OUT):
        try: done.add(json.loads(line)["id"])
        except Exception: pass
todo = [j for j in jobs if j["id"] not in done]
print(f"resume: {len(done)} done, {len(todo)} to go", flush=True)

# ---- workers ---------------------------------------------------------------
q = queue.Queue()
for j in todo: q.put(j)
lock = threading.Lock()
counters = {"ok": 0, "err": 0}

def ensure_server():
    try:
        urllib.request.urlopen("http://127.0.0.1:30878/v1/models", timeout=5); return True
    except Exception:
        subprocess.run(["sudo", "docker", "restart", "qwen-ab"], capture_output=True)
        for _ in range(120):
            time.sleep(5)
            try:
                urllib.request.urlopen("http://127.0.0.1:30878/v1/models", timeout=5)
                time.sleep(25)  # corpus background load
                return True
            except Exception: pass
        return False

def gen(job):
    d = make_general(job["query"]) if job["mode"] == "general" else make_request(job["query"], job["mode"])
    t0 = time.time()
    r = json.load(urllib.request.urlopen(urllib.request.Request(
        URL, data=json.dumps(d).encode(), headers={"Content-Type": "application/json"}), timeout=600))
    dt = time.time() - t0
    u = r.get("usage") or {}
    return {"id": job["id"], "family": job["family"], "mode": job["mode"], "query": job["query"],
            "content": r["choices"][0]["message"].get("content") or "",
            "completion_tokens": u.get("completion_tokens"), "prompt_tokens": u.get("prompt_tokens"),
            "dt": round(dt, 2), "ts": int(time.time())}

def worker():
    while True:
        try: job = q.get_nowait()
        except queue.Empty: return
        for attempt in range(4):
            try:
                rec = gen(job)
                with lock:
                    with open(OUT, "a") as f: f.write(json.dumps(rec) + "\n")
                    counters["ok"] += 1
                    n = counters["ok"]
                if n % 20 == 0:
                    print(f"[{time.strftime('%H:%M:%S')}] {n} done, {q.qsize()} queued, errs={counters['err']}", flush=True)
                break
            except Exception as e:
                with lock: counters["err"] += 1
                if attempt == 3:
                    print(f"GIVE UP {job['id']}: {type(e).__name__} {str(e)[:100]}", flush=True)
                else:
                    time.sleep(5 * (attempt + 1))
                    if attempt >= 1: ensure_server()
        q.task_done()

ensure_server()
threads = [threading.Thread(target=worker, daemon=True) for _ in range(WORKERS)]
for t in threads: t.start()
for t in threads: t.join()
print(f"HARVEST COMPLETE: ok={counters['ok']} errs={counters['err']} total_lines={sum(1 for _ in open(OUT))}", flush=True)
