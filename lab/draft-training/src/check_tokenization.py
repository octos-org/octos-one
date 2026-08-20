#!/usr/bin/env python3
"""CPU-only preflight for the teacher-forcing premise.

extract_hidden.py replays prompt+recorded-completion through the target. That is
only exact if our tokenization reproduces what the server actually tokenized.
Checks:
  - chat-templated prompt length vs the server's reported prompt_tokens
  - completion retokenization length vs reported completion_tokens
  - decode(encode(text)) == text  (byte-level round trip)
  - shared prompt prefix length per mode (drives the extraction phase split)
"""
import json, os, sys, collections

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_hidden import load_base, messages_for, chat_ids

BASE = "/home/ubuntu/qwen38-h200"


def main():
    tok_dir = sys.argv[1] if len(sys.argv) > 1 else "/models/target"
    limit = int(sys.argv[2]) if len(sys.argv) > 2 else 200
    from transformers import AutoTokenizer
    tok = AutoTokenizer.from_pretrained(tok_dir, trust_remote_code=True)
    print(f"tokenizer: vocab={tok.vocab_size} len={len(tok)}")
    for t in ("<|audio_start|>", "<|MASK|>"):
        print(f"  {t!r} -> {tok.convert_tokens_to_ids(t)}")

    base = load_base()
    recs = [json.loads(l) for l in open(os.path.join(BASE, "harvest", "out.jsonl")) if l.strip()]
    bym = collections.defaultdict(list)
    for r in recs:
        if (r.get("content") or "").strip():
            bym[r["mode"]].append(r)
    print({k: len(v) for k, v in bym.items()})

    prompts = collections.defaultdict(list)
    for mode, rs in bym.items():
        sub = rs[:limit]
        pdelta, cdelta, rt_bad = [], [], 0
        for r in sub:
            ids = chat_ids(tok, messages_for(base, r["query"], r["mode"]))
            prompts[mode].append(ids)
            cids = tok(r["content"], add_special_tokens=False)["input_ids"] + [tok.eos_token_id]
            pdelta.append(len(ids) - int(r.get("prompt_tokens") or 0))
            cdelta.append(len(cids) - int(r.get("completion_tokens") or 0))
            if tok.decode(cids) != r["content"]:
                rt_bad += 1
        pc = collections.Counter(pdelta)
        cc = collections.Counter(cdelta)
        print(f"\n[{mode}] n={len(sub)}")
        print(f"  prompt len delta   (ours - served): {dict(sorted(pc.items())[:6])}"
              f"  exact={pc.get(0,0)}/{len(sub)}")
        print(f"  completion delta   (ours - served): {dict(sorted(cc.items())[:6])}"
              f"  exact={cc.get(0,0)}/{len(sub)}")
        print(f"  decode(encode(text)) != text : {rt_bad}/{len(sub)}")

    for mode, ms in prompts.items():
        if len(ms) < 2:
            continue
        n = min(len(a) for a in ms)
        ref, lo = ms[0], 0
        while lo < n and all(a[lo] == ref[lo] for a in ms):
            lo += 1
        tail = tok.decode(ref[lo - 40:lo]) if lo >= 40 else ""
        print(f"\n[{mode}] shared prompt prefix = {lo} tokens of {len(ref)} "
              f"(divergence at token {lo}); 40 tokens before divergence: {tail!r}")


if __name__ == "__main__":
    main()
