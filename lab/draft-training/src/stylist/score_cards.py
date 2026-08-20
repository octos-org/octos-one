#!/usr/bin/env python3
"""Step 1: rank the harvested cards by DESIGN quality.

The production server on :30878 is the judge -- it is the same model that wrote
every card, so self-judge bias is symmetric across the set and only the RANKING
is used downstream. Temp 0, short structured output.

Also computes cheap structural metrics straight from the runl0 source, which are
model-free and act as a sanity check on the judge's ranking.

Writes src/stylist/scores.jsonl (one row per card record).
"""
from __future__ import annotations
import argparse, json, os, re, sys, threading, time, urllib.request, queue

BASE = "/home/ubuntu/qwen38-h200"
HARVEST = os.path.join(BASE, "harvest", "out.jsonl")

RUBRIC = """You are a strict design reviewer grading UI cards written in the runl0 declarative DSL.
Grade DESIGN QUALITY ONLY. Ignore whether data sources or values are factually correct;
ignore whether the DSL would compile. Judge it as a designer judges a rendered screen.

You are grading ONE card out of ~1100 produced by the SAME generator for the same app
catalogue. They are all competent and superficially similar. Your job is to SEPARATE them.
Use the FULL 0-100 range and be decisive about small differences -- a two-point difference
must mean something. Anchors: 50 = the exactly typical card of this corpus; 75 = clearly
better than most in a way you could name; 90+ = the showcase example; 25 = thin and flat.
Do not round to multiples of 5.

Dimensions, each 0-100:
  HIERARCHY  focal point, deliberate hero/title/caption/body scale, grouping, gap/pad rhythm.
  RICHNESS   genuinely distinct sections and section TYPES, reusable components, interaction
             (events, on_tap, state) that earns its place -- filler repetition does not count.
  THEME      coherent visual treatment: theme/mood/palette/typography, copy voice, eyebrow
             labels, localisation, consistent naming.
  IMAGERY    photography/icons/illustration/charts used as real design elements.
  DENSITY    information per screen, substantial but not cramped; loading/failed/empty states.

Then give SCORE, your overall 0-100 verdict -- not the mean, weighted toward what a person
would actually feel seeing the rendered card.

Reply with EXACTLY ONE line and nothing else:
HIERARCHY:<n> RICHNESS:<n> THEME:<n> IMAGERY:<n> DENSITY:<n> SCORE:<n>"""

LINE_RE = re.compile(
    r"HIERARCHY:\s*(\d+).*?RICHNESS:\s*(\d+).*?THEME:\s*(\d+).*?"
    r"IMAGERY:\s*(\d+).*?DENSITY:\s*(\d+).*?SCORE:\s*(\d+)", re.S | re.I)


# ------------------------------------------------------------------ structure
SECTION_KINDS = ("Photo", "Panel", "Col", "Row", "Grid", "List", "Card", "Stack")


def structural(card: str) -> dict:
    """Model-free metrics read directly off the runl0 source."""
    body = card
    if "```" in body:                       # strip the fence the model emits
        body = re.sub(r"^```[a-zA-Z0-9]*\n", "", body.strip())
        body = re.sub(r"\n```$", "", body)
    top = [l for l in body.splitlines()]
    def n_decl(kw):
        return sum(1 for l in top if re.match(rf"\s*{kw}\s+\S", l))
    views = [m.group(1) for m in re.finditer(r"^view\s+(\w+)\s", body, re.M)]
    comps = [m.group(1) for m in re.finditer(r"^component\s+(\w+)\s*\(", body, re.M)]
    # the root view's children name the card's top-level sections
    root = re.search(r"^view\s+root\s+.*?\{(.*?)^\}", body, re.M | re.S)
    root_children = []
    if root:
        for l in root.group(1).splitlines():
            l = l.strip()
            m = re.match(r"^(\w+)\s*$", l)
            if m and m.group(1) not in ("current",) or (m and m.group(1) == "current"):
                root_children.append(m.group(1))
    widget_kinds = set(re.findall(r"\b([A-Z][A-Za-z0-9]+)\s*\(", body))
    return {
        "n_source": n_decl("source"),
        "n_state": n_decl("state"),
        "n_event": n_decl("event"),
        "n_copy": n_decl("copy"),
        "n_view": len(views),
        "n_component": len(comps),
        "n_root_sections": len(root_children),
        "n_widget_kinds": len(widget_kinds),
        "has_theme": bool(re.search(r"^\s*(theme|style|palette)\s+\S", body, re.M)),
        "n_image": len(re.findall(r"sys\.photo|\bPhoto\(|\bImage\(|\bThumb\w*\(|Icon\(", body)),
        "n_when": len(re.findall(r"\bwhen\b", body)),
        "n_for": len(re.findall(r"^\s*for\s+\w+\s+in\s", body, re.M)),
        "has_pending": "$state == .pending" in body,
        "has_failed": "$state == .failed" in body,
        "n_i18n": len(re.findall(r"\bzh:\s*\"", body)),
        "n_lines": len(top),
        "n_chars": len(body),
    }


# ---------------------------------------------------------------------- judge
def post(port, msgs, max_tokens, timeout=600, retries=3):
    body = {"model": "x", "messages": msgs, "max_tokens": max_tokens,
            "temperature": 0, "stream": False}
    last = None
    for a in range(retries):
        try:
            req = urllib.request.Request(
                f"http://127.0.0.1:{port}/v1/chat/completions",
                data=json.dumps(body).encode(),
                headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req, timeout=timeout) as r:
                d = json.load(r)
            return d["choices"][0]["message"]["content"] or ""
        except Exception as e:
            last = e
            time.sleep(2 + 3 * a)
    raise last


def judge_one(port, rec):
    msgs = [{"role": "user", "content":
             RUBRIC + "\n\nUSER REQUEST THE CARD ANSWERS:\n" + rec["query"]
             + "\n\nCARD:\n" + rec["content"]}]
    txt = post(port, msgs, max_tokens=48)
    m = LINE_RE.search(txt)
    if not m:
        return None, txt
    k = ("hierarchy", "richness", "theme", "imagery", "density", "score")
    return {a: int(b) for a, b in zip(k, m.groups())}, txt


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=30878)
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                  "scores.jsonl"))
    ap.add_argument("--concurrency", type=int, default=3)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--feat-index", default="/mnt/dflash-feats/index.json")
    args = ap.parse_args()

    recs = [json.loads(l) for l in open(HARVEST) if l.strip()]
    cards = [r for r in recs if r["mode"] in ("pick", "compose")
             and (r.get("content") or "").strip()]
    usable = set()
    if os.path.exists(args.feat_index):
        usable = {e["name"] for e in json.load(open(args.feat_index))
                  if e["kind"] == "seq" and e.get("contiguous") and e.get("aligned")}

    done = {}
    if os.path.exists(args.out):
        for l in open(args.out):
            try:
                d = json.loads(l)
                done[d["id"]] = d
            except Exception:
                pass
        print(f"[score] resuming, {len(done)} already scored")
    todo = [r for r in cards if r["id"] not in done]
    if args.limit:
        todo = todo[: args.limit]
    print(f"[score] {len(cards)} cards, {len(todo)} to judge, "
          f"{len(usable & {c['id'] for c in cards})} of them have features")

    q = queue.Queue()
    for r in todo:
        q.put(r)
    lock = threading.Lock()
    f = open(args.out, "a")
    t0 = time.time()
    state = {"n": 0, "bad": 0}

    def worker():
        while True:
            try:
                rec = q.get_nowait()
            except queue.Empty:
                return
            try:
                sc, raw = judge_one(args.port, rec)
            except Exception as e:
                sc, raw = None, f"ERROR {e}"
            row = {"id": rec["id"], "mode": rec["mode"], "family": rec["family"],
                   "query": rec["query"], "completion_tokens": rec["completion_tokens"],
                   "has_feats": rec["id"] in usable,
                   "judge": sc, "raw": None if sc else raw[:200],
                   "struct": structural(rec["content"])}
            with lock:
                f.write(json.dumps(row) + "\n"); f.flush()
                state["n"] += 1
                if sc is None:
                    state["bad"] += 1
                if state["n"] % 25 == 0:
                    el = time.time() - t0
                    print(f"  {state['n']}/{len(todo)}  {el:.0f}s  "
                          f"{el/max(1,state['n']):.2f}s/card  unparsed={state['bad']}",
                          flush=True)

    ths = [threading.Thread(target=worker, daemon=True) for _ in range(args.concurrency)]
    for t in ths: t.start()
    for t in ths: t.join()
    f.close()
    print(f"[score] done: {state['n']} judged in {time.time()-t0:.0f}s, "
          f"{state['bad']} unparsed -> {args.out}")


if __name__ == "__main__":
    main()
