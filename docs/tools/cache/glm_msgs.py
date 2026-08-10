import json, os, time, urllib.request, urllib.error
from cache_experiment import baseline, personal_delta
KEY=os.environ["ZAI_API_KEY"]; URL="https://api.z.ai/api/anthropic/v1/messages"
def call(messages, retries=3):
    for a in range(retries):
        try:
            req=urllib.request.Request(URL,method="POST",
                data=json.dumps({"model":"glm-5.2","max_tokens":8,"messages":messages}).encode(),
                headers={"x-api-key":KEY,"anthropic-version":"2023-06-01","content-type":"application/json"})
            with urllib.request.urlopen(req,timeout=240) as r: return json.loads(r.read())["usage"]
        except urllib.error.HTTPError as e:
            if a==retries-1: print("  HTTP",e.code,e.read().decode()[:150]); raise
            time.sleep(8)
def row(l,u,note=""):
    f=u.get("input_tokens",0); c=u.get("cache_read_input_tokens",0); t=f+c
    print(f"  {l:<38} total {t:>6}  cached {c:>6}  hit {100.0*c/t if t else 0:5.1f}%  {note}")
base=baseline()
seg1=[{"role":"user","content":base},{"role":"assistant","content":"ok"}]
print("GLM-5.2 — ledger as a message sequence, appends as NEW messages\n")
row("1. seg1 + request (cold)", call(seg1+[{"role":"user","content":"Go."}]))
time.sleep(6); row("2. same, repeat", call(seg1+[{"role":"user","content":"Go."}]), "warm")
time.sleep(6)
grown=seg1+[{"role":"user","content":"Go."},{"role":"assistant","content":"done"},
            {"role":"user","content":personal_delta("A")}]
row("3. APPENDED as a new message", call(grown), "<- prefix survive?")
time.sleep(6)
row("4. APPENDED again", call(grown+[{"role":"assistant","content":"done"},
                                     {"role":"user","content":personal_delta("B")}]), "<- again?")
