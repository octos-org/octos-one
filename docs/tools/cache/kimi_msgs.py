import json, os, time, urllib.request, urllib.error
from cache_experiment import baseline, personal_delta
KEY=os.environ["MOONSHOT_API_KEY"]; URL="https://api.moonshot.ai/v1/chat/completions"

def call(messages, retries=3):
    for a in range(retries):
        try:
            req=urllib.request.Request(URL,method="POST",
                data=json.dumps({"model":"kimi-k3","max_tokens":8,"messages":messages}).encode(),
                headers={"Authorization":f"Bearer {KEY}","Content-Type":"application/json"})
            with urllib.request.urlopen(req,timeout=240) as r: return json.loads(r.read())["usage"]
        except urllib.error.HTTPError as e:
            if a==retries-1: print("  HTTP",e.code,e.read().decode()[:150]); raise
            time.sleep(8)

def row(l,u,note=""):
    p=u.get("prompt_tokens",0); c=u.get("cached_tokens",0)
    print(f"  {l:<38} prompt {p:>6}  cached {c:>6}  hit {100.0*c/p if p else 0:5.1f}%  {note}")

base=baseline()
# Ledger as a MESSAGE SEQUENCE: sealed segment(s) first, appends as new messages.
seg1=[{"role":"user","content":base},{"role":"assistant","content":"ok"}]
print("Ledger as a message sequence — appends are NEW messages\n")
row("1. seg1 + request (cold)", call(seg1+[{"role":"user","content":"Go."}]))
time.sleep(6)
row("2. same, repeat", call(seg1+[{"role":"user","content":"Go."}]), "warm baseline")
time.sleep(6)
# APPEND: previous turns unchanged, a new message added at the end
grown = seg1+[{"role":"user","content":"Go."},{"role":"assistant","content":"done"},
              {"role":"user","content":personal_delta("A")}]
row("3. APPENDED as a new message", call(grown), "<- does the prefix survive?")
time.sleep(6)
grown2 = grown+[{"role":"assistant","content":"done"},{"role":"user","content":personal_delta("B")}]
row("4. APPENDED again", call(grown2), "<- and again?")
time.sleep(6)
row("5. control: edit inside seg1",
    call([{"role":"user","content":base.replace("@0005 source","@0005 SOURCE",1)},
          {"role":"assistant","content":"ok"},{"role":"user","content":"Go."}]), "<- should collapse")
