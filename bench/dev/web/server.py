#!/usr/bin/env python3
"""Local site for the dev web tasks: a clinic, an ops table, a developer console, a help desk, a staff directory and
a blog CMS.

127.0.0.1 only. Every value comes from tasks_dev.world(seed), set by GET /__reset?seed=N. Every write (booking,
token revoke, preference save, desk update, post save) and every page view is recorded, and GET /__state returns
them, so verifiers judge by what the site saw, not by the agent's claim. Python stdlib only.

Usage: python3 -I -B dev/web/server.py [port]   (default 8794)
"""
import datetime
import html
import json
import math
import re
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlparse

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import tasks_dev as td  # noqa: E402

LOCK = threading.Lock()
STATE = {}
CACHE = {}
TODAY = datetime.date(2026, 11, 16)
e = html.escape


def reset(seed):
    w = td.world(seed)
    with LOCK:
        STATE.clear()
        STATE.update({"seed": seed, "navs": [], "events": [], "appointments": [], "revoked": [],
                      "prefs": json.loads(json.dumps(w["dw4"]["initial"])), "prefs_saves": 0, "desk_posts": [],
                      "desk_status": {str(t["id"]): t["status"] for t in w["dw5"]["tickets"]},
                      "posts": {str(p["id"]): json.loads(json.dumps(p)) for p in w["dw7"]["posts"]}})
        CACHE.clear()
        CACHE["w"] = w


reset(0)

SECTIONS = [("clinic", "Clinic"), ("ops/shipments", "Shipments"), ("console/tokens", "Console"), ("desk", "Help desk"),
            ("people", "People"), ("cms", "CMS")]
CSS = ("body{font:15px system-ui,sans-serif;margin:0;color:#222}main{margin:1.2em 1.5em;max-width:1100px}"
       "header{background:#24323f;color:#fff;padding:.6em 1.5em}header a{color:#cfe3ff;margin-right:1em}"
       "td,th{padding:5px 8px;border-bottom:1px solid #ddd;text-align:left;vertical-align:top}"
       "label{display:block;margin:.6em 0}.err{color:#a00}.errors{border:1px solid #a00;background:#fee;padding:.5em 1em}"
       ".chip{display:inline-block;background:#e6eef7;border-radius:10px;padding:0 6px;margin:2px}"
       ".muted{color:#777}.card{border:1px solid #ddd;border-radius:6px;padding:.6em 1em;margin:.5em 0}"
       ".backdrop{position:fixed;inset:0;background:rgba(0,0,0,.45);display:flex;align-items:center;justify-content:center}"
       ".backdrop[hidden]{display:none}.dialog{background:#fff;padding:1.2em 1.5em;border-radius:8px;width:420px}"
       ".cols{display:flex;gap:1.5em}.side{width:260px;font-size:13px}.grow{flex:1}"
       "[role=listbox]{border:1px solid #999;max-height:180px;overflow:auto;margin:0;padding:0;list-style:none;width:320px;"
       "background:#fff;position:absolute}[role=option]{padding:3px 8px;cursor:pointer}"
       "[role=option][aria-selected=true]{background:#cfe3ff}")


def page(title, body, head="", bare=False):
    nav = "".join(f"<a href=/{s}/>{n}</a>" if not s.startswith(("ops", "console")) else f"<a href=/{s}>{n}</a>"
                  for s, n in SECTIONS)
    top = "" if bare else f"<header><b>Northwind Intranet</b> &nbsp; {nav}</header>"
    return (f"<!doctype html><html lang=en><head><meta charset=utf-8><title>{e(title)}</title><style>{CSS}</style>"
            f"{head}</head><body>{top}<main><h1>{e(title)}</h1>{body}</main></body></html>")


def data_script(id_, value):
    return f"<script type=application/json id={id_}>{json.dumps(value).replace('<', chr(92) + 'u003c')}</script>"


def as_int(value, default=1):
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def pager(base, query, page_no, pages, key="p"):
    links = []
    for n in range(1, pages + 1):
        q = dict(query, **{key: str(n)})
        links.append(f"<b>{n}</b>" if n == page_no else f"<a href='{base}?{e(urlencode(q))}'>{n}</a>")
    return f"<nav aria-label=Pages><p>Page: {' '.join(links)}</p></nav>" if pages > 1 else ""


def chunk(items, page_no, size):
    pages = max(1, math.ceil(len(items) / size))
    page_no = min(max(1, page_no), pages)
    return items[(page_no - 1) * size: page_no * size], page_no, pages


def short_date(iso):
    d = datetime.date.fromisoformat(iso)
    return f"{d.strftime('%b')} {d.day}, {d.year}"


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def send(self, status, body, ctype="text/html; charset=utf-8", extra=None):
        data = body.encode() if isinstance(body, str) else body
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def redirect(self, to):
        self.send(303, "", extra={"Location": to})

    def json_reply(self, status, value):
        self.send(status, json.dumps(value), "application/json")

    def do_GET(self):
        url = urlparse(self.path)
        q = {k: v[0] for k, v in parse_qs(url.query).items()}
        p = url.path
        if p == "/__reset":
            reset(as_int(q.get("seed"), 0))
            return self.json_reply(200, td.reset_payload(STATE["seed"]))
        if p == "/__state":
            with LOCK:
                return self.json_reply(200, STATE)
        if p == "/favicon.ico":
            return self.send(404, "")
        with LOCK:
            STATE["navs"].append({"path": p, "query": q})
        w = CACHE["w"]
        for prefix, fn in (("/clinic", self.clinic), ("/ops", self.ops), ("/console", self.console),
                           ("/desk", self.desk), ("/people", self.people), ("/cms", self.cms)):
            if p == prefix or p.startswith(prefix + "/"):
                return fn(p[len(prefix):] or "/", q, w)
        if p == "/":
            links = "".join(f"<li><a href=/{s}>{n}</a></li>" for s, n in SECTIONS)
            return self.send(200, page("Northwind Intranet", f"<ul>{links}</ul>"))
        return self.not_found()

    def not_found(self):
        return self.send(404, page("Not found", "<p>No such page.</p>"))

    def read_body(self):
        n = int(self.headers.get("Content-Length", "0") or 0)
        return self.rfile.read(n).decode()

    def do_POST(self):
        url = urlparse(self.path)
        p = url.path
        raw = self.read_body()
        if (self.headers.get("Content-Type") or "").startswith("application/json"):
            try:
                f = json.loads(raw or "{}")
            except ValueError:
                return self.json_reply(400, {"error": "bad json"})
            if not isinstance(f, dict):
                return self.json_reply(400, {"error": "bad json"})
        else:
            f = {k: v[0] if len(v) == 1 else v for k, v in parse_qs(raw, keep_blank_values=True).items()}
        with LOCK:
            STATE["events"].append({"kind": p, "fields": f, "t": datetime.datetime.now().isoformat()})
        w = CACHE["w"]
        if p == "/clinic/book":
            return self.clinic_book(f, w)
        if p == "/console/tokens/revoke":
            return self.revoke(f, w)
        if p == "/console/notifications/save":
            return self.save_prefs(f)
        m = re.fullmatch(r"/desk/ticket/(\d+)/composer", p)
        if m:
            return self.desk_post(int(m.group(1)), f, w)
        m = re.fullmatch(r"/cms/posts/(\d+)/edit", p)
        if m:
            return self.cms_save(m.group(1), f, w)
        return self.not_found()

    # ------------------------------------------------------------------ clinic

    def clinic(self, p, q, w):
        if p == "/":
            body = ("<p>Northwind Family Clinic. Open Monday to Friday, 8:00 AM to 5:00 PM. Appointments start on the "
                    "quarter hour.</p><p><a href=/clinic/book>Book an appointment</a> · <a href=/clinic/doctors>Our "
                    "doctors</a></p><h2>News</h2><p>Flu shots are available without an appointment on Tuesdays.</p>")
            return self.send(200, page("Northwind Family Clinic", body))
        if p == "/doctors":
            rows = "".join(f"<tr><td>{e(d['name'])}</td><td>{e(d['dept'])}</td></tr>" for d in w["dw1"]["doctors"])
            return self.send(200, page("Our doctors", f"<table><tr><th>Doctor</th><th>Department</th></tr>{rows}</table>"))
        if p == "/book":
            return self.send(200, self.booking_form(w, {}, {}))
        m = re.fullmatch(r"/booked/(\d+)", p)
        if m and int(m.group(1)) < len(STATE["appointments"]):
            a = STATE["appointments"][int(m.group(1))]
            doc = next(d for d in w["dw1"]["doctors"] if d["id"] == a["doctor"])
            return self.send(200, page("Appointment booked", f"<p>{e(a['patient'])} with {e(doc['name'])} on "
                                       f"{short_date(a['date'])} at {td.clock12(a['time'])}.</p>"))
        return self.not_found()

    def booking_form(self, w, v, errs):
        c = w["dw1"]
        depts = "".join(f"<option{' selected' if v.get('department') == d else ''}>{e(d)}</option>" for d in td.DEPARTMENTS)
        docs = "".join(f"<option value={d['id']} data-dept='{e(d['dept'])}'{' selected' if v.get('doctor') == d['id'] else ''}>"
                       f"{e(d['name'])}</option>" for d in c["doctors"])

        def err(k):
            return f"<span class=err id=err-{k}>{e(errs[k])}</span>" if k in errs else ""

        def inv(k):
            return f" aria-invalid=true aria-describedby=err-{k}" if k in errs else ""

        summary = ""
        if errs:
            summary = ("<div class=errors role=alert><p><b>Please fix the following:</b></p><ul>" +
                       "".join(f"<li>{e(m)}</li>" for m in errs.values()) + "</ul></div>")
        val = lambda k: e(str(v.get(k, "")))  # noqa: E731
        body = (f"{summary}<form method=post action=/clinic/book novalidate>"
                f"<label>Patient full name <input name=patient value='{val('patient')}'{inv('patient')}></label>{err('patient')}"
                f"<label>Date of birth <input type=date name=dob value='{val('dob')}'{inv('dob')}></label>{err('dob')}"
                f"<label>Phone <input name=phone value='{val('phone')}'{inv('phone')}></label>{err('phone')}"
                f"<label>Department <select name=department id=department{inv('department')}><option value=''>Choose a "
                f"department</option>{depts}</select></label>{err('department')}"
                f"<label>Doctor <select name=doctor id=doctor{inv('doctor')}><option value=''>Choose a doctor</option>"
                f"{docs}</select></label>{err('doctor')}"
                f"<label>Appointment date <input type=date name=date value='{val('date')}'{inv('date')}></label>{err('date')}"
                f"<label>Time <input type=time name=time min=08:00 max=16:45 step=900 value='{val('time')}'{inv('time')}>"
                f"</label>{err('time')}"
                f"<label>Reason for the visit <textarea name=reason rows=3 cols=50{inv('reason')}>{val('reason')}</textarea>"
                f"</label>{err('reason')}"
                f"<label><input type=checkbox name=consent value=yes{' checked' if v.get('consent') else ''}{inv('consent')}> "
                f"I confirm these details are correct</label>{err('consent')}"
                "<p><button>Book appointment</button></p></form>"
                "<script>(function(){var dep=document.getElementById('department'),doc=document.getElementById('doctor');"
                "function sync(){var d=dep.value;doc.disabled=!d;for(var o of doc.options){if(!o.value)continue;"
                "o.hidden=o.dataset.dept!==d;if(o.hidden&&o.selected)doc.value='';}}"
                "dep.addEventListener('change',sync);sync();})();</script>")
        return page("Book an appointment", body)

    def clinic_book(self, f, w):
        c = w["dw1"]
        v = {k: str(f.get(k, "")).strip() for k in ("patient", "dob", "phone", "department", "doctor", "date", "time",
                                                     "reason", "consent")}
        errs = {}
        if not v["patient"]:
            errs["patient"] = "Enter the patient's full name."
        try:
            if datetime.date.fromisoformat(v["dob"]) >= TODAY:
                errs["dob"] = "Date of birth must be in the past."
        except ValueError:
            errs["dob"] = "Enter a valid date of birth."
        if not re.fullmatch(r"\d{3}-\d{3}-\d{4}", v["phone"]):
            errs["phone"] = "Phone number must be written as 555-123-4567 (digits and dashes only)."
        if v["department"] not in td.DEPARTMENTS:
            errs["department"] = "Choose a department."
        doc = next((d for d in c["doctors"] if d["id"] == v["doctor"]), None)
        if not doc or doc["dept"] != v["department"]:
            errs["doctor"] = "Choose a doctor from the selected department."
        day = None
        try:
            day = datetime.date.fromisoformat(v["date"])
            if day <= TODAY:
                errs["date"] = "Choose a date after today."
            elif day.weekday() >= 5:
                errs["date"] = "The clinic is closed on weekends."
        except ValueError:
            errs["date"] = "Enter a valid appointment date."
        tm = v["time"][:5]
        if tm not in td.quarter_slots():
            errs["time"] = "Appointments start on the quarter hour between 8:00 AM and 4:45 PM."
        elif doc and day and "date" not in errs:
            taken = self.taken(c, doc["id"], day.isoformat())
            if tm in taken:
                free = [td.clock12(s) for s in td.quarter_slots() if s not in taken]
                errs["time"] = (f"{doc['name']} is already booked at {td.clock12(tm)} on {short_date(day.isoformat())}. "
                                f"Free times that day: {', '.join(free)}.")
        if len(v["reason"]) < 5:
            errs["reason"] = "Tell us briefly why you are coming in."
        if not v["consent"]:
            errs["consent"] = "Please confirm the details are correct."
        if errs:
            return self.send(200, self.booking_form(w, v, errs))
        with LOCK:
            STATE["appointments"].append({"patient": v["patient"], "dob": v["dob"], "phone": v["phone"],
                                          "department": v["department"], "doctor": v["doctor"], "date": v["date"],
                                          "time": tm, "reason": v["reason"]})
            n = len(STATE["appointments"]) - 1
        return self.redirect(f"/clinic/booked/{n}")

    def taken(self, c, doctor, day):
        out = set(c["booked"]) if (doctor, day) == (c["doctor"]["id"], c["day"]) else set()
        out |= {a["time"] for a in STATE["appointments"] if (a["doctor"], a["date"]) == (doctor, day)}
        return out

    # ------------------------------------------------------------------ ops

    def ops(self, p, q, w):
        if p not in ("/shipments", "/shipments/"):
            return self.not_found()
        statuses = "".join(f"<option>{e(s)}</option>" for s in td.STATUSES)
        cols = [("tracking", "Tracking #"), ("carrier", "Carrier"), ("dest", "Destination"), ("status", "Status"),
                ("shipped", "Shipped"), ("weight", "Weight")]
        heads = "".join(f"<th aria-sort=none data-key={k}><button type=button>{n}</button></th>" for k, n in cols)
        body = (f"<p>All outbound shipments this quarter. Click a column header to sort; click again to reverse.</p>"
                "<p><label style=display:inline>Filter <input id=filter type=search placeholder='Filter shipments' "
                "aria-label='Filter shipments'></label> <label style=display:inline>Status <select id=status>"
                f"<option value=''>All statuses</option>{statuses}</select></label></p>"
                f"<p id=count aria-live=polite></p><table id=grid><thead><tr>{heads}</tr></thead><tbody></tbody></table>"
                "<p><button type=button id=prev>Previous</button> <span id=pageinfo></span> "
                "<button type=button id=next>Next</button></p>" + data_script("rows", w["dw2"]["rows"]) + """
<script>(function(){
var rows=JSON.parse(document.getElementById('rows').textContent),key='shipped',dir=-1,pg=0,size=20;
var f=document.getElementById('filter'),st=document.getElementById('status'),tb=document.querySelector('#grid tbody');
function shown(){var t=f.value.trim().toLowerCase(),s=st.value;return rows.filter(function(r){
 return (!s||r.status===s)&&(!t||[r.tracking,r.carrier,r.dest,r.status,r.shipped,String(r.weight)].join(' ').toLowerCase().indexOf(t)>=0);
}).sort(function(a,b){var x=a[key],y=b[key];return (x<y?-1:x>y?1:0)*dir;});}
function draw(){var all=shown(),pages=Math.max(1,Math.ceil(all.length/size));if(pg>=pages)pg=pages-1;
 tb.innerHTML='';all.slice(pg*size,pg*size+size).forEach(function(r){var tr=document.createElement('tr');
 [r.tracking,r.carrier,r.dest,r.status,r.shipped,r.weight.toFixed(1)+' kg'].forEach(function(v){var td=document.createElement('td');td.textContent=v;tr.appendChild(td);});tb.appendChild(tr);});
 document.getElementById('count').textContent=all.length+' shipments';
 document.getElementById('pageinfo').textContent='Page '+(pg+1)+' of '+pages;
 document.getElementById('prev').disabled=pg===0;document.getElementById('next').disabled=pg>=pages-1;
 document.querySelectorAll('#grid th').forEach(function(th){th.setAttribute('aria-sort',th.dataset.key===key?(dir>0?'ascending':'descending'):'none');});}
document.querySelectorAll('#grid th button').forEach(function(b){b.addEventListener('click',function(){
 var k=b.parentNode.dataset.key;if(k===key)dir=-dir;else{key=k;dir=1;}pg=0;draw();});});
f.addEventListener('input',function(){pg=0;draw();});st.addEventListener('change',function(){pg=0;draw();});
document.getElementById('prev').addEventListener('click',function(){pg--;draw();});
document.getElementById('next').addEventListener('click',function(){pg++;draw();});draw();})();</script>""")
        return self.send(200, page("Shipments", body))

    # ------------------------------------------------------------------ console

    def console(self, p, q, w):
        if p in ("/", ""):
            return self.send(200, page("Developer console", "<ul><li><a href=/console/tokens>API tokens</a><li>"
                                       "<a href=/console/notifications>Notification settings</a></ul>"))
        if p == "/tokens":
            return self.tokens_page(w)
        if p == "/notifications":
            return self.prefs_page()
        return self.not_found()

    def tokens_page(self, w):
        live = [t for t in w["dw3"]["tokens"] if t["id"] not in STATE["revoked"]]
        rows = "".join(f"<tr data-id={t['id']}><td><b>{e(t['name'])}</b></td><td>" +
                       "".join(f"<code class=chip>{e(s)}</code>" for s in t["scopes"]) +
                       f"</td><td>{short_date(t['created'])}</td><td>{short_date(t['used']) if t['used'] else 'Never'}</td>"
                       f"<td><button type=button class=revoke data-id={t['id']} data-name='{e(t['name'])}'>Revoke</button>"
                       "</td></tr>" for t in live)
        body = ("<p>Personal access tokens let scripts call the API as you. Revoked tokens stop working at once.</p>"
                "<p id=flash role=status></p><table><thead><tr><th>Name</th><th>Scopes</th><th>Created</th>"
                f"<th>Last used</th><th></th></tr></thead><tbody>{rows}</tbody></table>"
                "<div class=backdrop id=modal hidden><div class=dialog role=dialog aria-modal=true aria-labelledby=mt>"
                "<h2 id=mt>Revoke token?</h2><p>This cannot be undone. Type <b id=mname></b> to confirm.</p>"
                "<p><input id=mconfirm aria-label='Token name' autocomplete=off></p><p id=merr class=err></p>"
                "<p><button type=button id=mcancel>Cancel</button> <button type=button id=mgo disabled>Revoke token"
                "</button></p></div></div>" + """
<script>(function(){
var modal=document.getElementById('modal'),inp=document.getElementById('mconfirm'),go=document.getElementById('mgo'),cur=null;
function close(){modal.hidden=true;cur=null;inp.value='';go.disabled=true;document.getElementById('merr').textContent='';}
document.querySelectorAll('button.revoke').forEach(function(b){b.addEventListener('click',function(){
 cur=b;document.getElementById('mname').textContent=b.dataset.name;modal.hidden=false;inp.focus();});});
inp.addEventListener('input',function(){go.disabled=inp.value!==(cur&&cur.dataset.name);});
document.getElementById('mcancel').addEventListener('click',close);
document.addEventListener('keydown',function(ev){if(ev.key==='Escape'&&!modal.hidden)close();});
go.addEventListener('click',function(){var b=cur;fetch('/console/tokens/revoke',{method:'POST',headers:{'Content-Type':'application/json'},
 body:JSON.stringify({id:Number(b.dataset.id),confirm:inp.value})}).then(function(r){return r.json().then(function(j){
 if(!r.ok){document.getElementById('merr').textContent=j.error||'Could not revoke';return;}
 b.closest('tr').remove();document.getElementById('flash').textContent='Token '+b.dataset.name+' revoked.';close();});});});
})();</script>""")
        return self.send(200, page("API tokens", body))

    def revoke(self, f, w):
        tok = next((t for t in w["dw3"]["tokens"] if t["id"] == as_int(f.get("id"), 0)), None)
        if not tok or tok["id"] in STATE["revoked"]:
            return self.json_reply(404, {"error": "No such token."})
        if f.get("confirm") != tok["name"]:
            return self.json_reply(400, {"error": "The name does not match."})
        with LOCK:
            STATE["revoked"].append(tok["id"])
        return self.json_reply(200, {"ok": True})

    def prefs_page(self):
        body = ("<p>Choose how we reach you. Changes apply after you save.</p><notif-matrix></notif-matrix>"
                "<h2>Help</h2><p class=muted>SMS uses the phone number on your profile. Push needs the mobile app.</p>"
                + data_script("prefs", {"events": td.EVENTS, "channels": td.CHANNELS, "prefs": STATE["prefs"]}) + """
<script>(function(){
var data=JSON.parse(document.getElementById('prefs').textContent);
customElements.define('notif-matrix',class extends HTMLElement{connectedCallback(){
 var root=this.attachShadow({mode:'open'}),prefs=data.prefs;
 var css='<style>table{border-collapse:collapse}td,th{padding:6px 10px;border-bottom:1px solid #ddd}'+
  'button[role=switch]{width:46px;border-radius:12px;border:1px solid #888;background:#eee}'+
  'button[aria-checked=true]{background:#2a7;color:#fff}</style>';
 var head='<tr><th>Event</th>'+data.channels.map(function(c){return '<th>'+c+'</th>';}).join('')+'</tr>';
 var body=data.events.map(function(ev){return '<tr><td>'+ev+'</td>'+data.channels.map(function(c){
  var on=!!prefs[ev][c];return '<td><button type=button role=switch aria-checked='+on+' data-ev="'+ev+'" data-ch="'+c+
  '" aria-label="'+ev+': '+c+'">'+(on?'On':'Off')+'</button></td>';}).join('')+'</tr>';}).join('');
 root.innerHTML=css+'<table>'+head+body+'</table><p><button type=button id=save>Save preferences</button> '+
  '<span id=msg role=status></span></p>';
 root.querySelectorAll('button[role=switch]').forEach(function(b){b.addEventListener('click',function(){
  var on=b.getAttribute('aria-checked')!=='true';b.setAttribute('aria-checked',String(on));b.textContent=on?'On':'Off';
  prefs[b.dataset.ev][b.dataset.ch]=on;root.getElementById('msg').textContent='Unsaved changes';});});
 root.getElementById('save').addEventListener('click',function(){fetch('/console/notifications/save',{method:'POST',
  headers:{'Content-Type':'application/json'},body:JSON.stringify({prefs:prefs})}).then(function(r){
  root.getElementById('msg').textContent=r.ok?'Preferences saved.':'Could not save.';});});
}});})();</script>""")
        return self.send(200, page("Notification settings", body))

    def save_prefs(self, f):
        prefs = f.get("prefs") if isinstance(f, dict) else None
        if not isinstance(prefs, dict) or set(prefs) != set(td.EVENTS) or \
                any(not isinstance(prefs[x], dict) or set(prefs[x]) != set(td.CHANNELS) for x in td.EVENTS):
            return self.json_reply(400, {"error": "bad preferences"})
        with LOCK:
            STATE["prefs"] = {x: {c: bool(prefs[x][c]) for c in td.CHANNELS} for x in td.EVENTS}
            STATE["prefs_saves"] += 1
        return self.json_reply(200, {"ok": True})

    # ------------------------------------------------------------------ desk

    def desk(self, p, q, w):
        tickets = w["dw5"]["tickets"]
        status = STATE["desk_status"]
        if p == "/":
            term = q.get("q", "").strip().lower()
            hits = [t for t in tickets if all(x in f"{t['subject']} {t['requester']} {t['id']}".lower() for x in term.split())]
            items, page_no, pages = chunk(hits, as_int(q.get("p")), 15)
            rows = "".join(f"<tr><td><a href=/desk/ticket/{t['id']}>#{t['id']}</a></td><td>{e(t['subject'])}</td>"
                           f"<td>{e(t['requester'])}</td><td>{e(status[str(t['id'])])}</td>"
                           f"<td>{t['updated'].replace('T', ' ')}</td></tr>" for t in items)
            body = (f"<form action=/desk/><input name=q value='{e(q.get('q', ''))}' placeholder='Search tickets' "
                    "aria-label='Search tickets'> <button>Search</button></form>"
                    f"<p>{len(hits)} tickets</p><table><tr><th>#</th><th>Subject</th><th>Requester</th><th>Status</th>"
                    f"<th>Updated</th></tr>{rows}</table>{pager('/desk/', q, page_no, pages)}")
            return self.send(200, page("Tickets", body))
        m = re.fullmatch(r"/ticket/(\d+)(/composer)?", p)
        t = next((t for t in tickets if m and t["id"] == int(m.group(1))), None)
        if not t:
            return self.not_found()
        if m.group(2):
            return self.composer(t, q)
        thread = [f"<div class=card><p><b>{e(t['requester'])}</b> <span class=muted>{t['updated'].replace('T', ' ')}"
                  f"</span></p><p>{e(t['first'])}</p></div>"]
        for x in STATE["desk_posts"]:
            if x["ticket"] == t["id"]:
                label = "Internal note" if x["kind"] == "internal" else "Public reply"
                thread.append(f"<div class=card><p><b>You</b> · {label}</p><p>{e(x['body'])}</p></div>")
        body = (f"<p>Requester: <b>{e(t['requester'])}</b> · Status: <b>{e(status[str(t['id'])])}</b> · "
                f"<a href=/desk/>Back to tickets</a></p>{''.join(thread)}<h2>Respond</h2>"
                f"<iframe src=/desk/ticket/{t['id']}/composer title='Reply composer' style='width:100%;height:330px;"
                "border:1px solid #ccc'></iframe>")
        return self.send(200, page(f"#{t['id']} {t['subject']}", body))

    def composer(self, t, q, err=""):
        cur = STATE["desk_status"][str(t["id"])]
        opts = "".join(f"<option{' selected' if s == cur else ''}>{s}</option>" for s in td.DESK_STATUSES)
        msg = f"<p role=status>Update sent. Status: {e(cur)}.</p>" if q.get("sent") else ""
        body = (f"{msg}<p class=err>{e(err)}</p><form method=post action=/desk/ticket/{t['id']}/composer>"
                "<fieldset><legend>Type</legend><label style=display:inline><input type=radio name=kind value=public "
                "checked> Public reply</label> <label style=display:inline><input type=radio name=kind value=internal> "
                "Internal note</label></fieldset><label>Message <textarea name=body rows=4 cols=70></textarea></label>"
                f"<label>Status <select name=status>{opts}</select></label><button>Submit</button></form>")
        return self.send(200, page("Composer", body, bare=True))

    def desk_post(self, tid, f, w):
        t = next((t for t in w["dw5"]["tickets"] if t["id"] == tid), None)
        if not t:
            return self.not_found()
        body, kind, new = str(f.get("body", "")).strip(), f.get("kind"), f.get("status")
        if kind not in ("public", "internal") or new not in td.DESK_STATUSES:
            return self.composer(t, {}, "Choose a type and a status.")
        if not body and new == STATE["desk_status"][str(tid)]:
            return self.composer(t, {}, "Write a message or change the status.")
        with LOCK:
            if body:
                STATE["desk_posts"].append({"ticket": tid, "kind": kind, "body": body})
            STATE["desk_status"][str(tid)] = new
        return self.redirect(f"/desk/ticket/{tid}/composer?sent=1")

    # ------------------------------------------------------------------ people

    def people(self, p, q, w):
        d = w["dw6"]
        everyone = d["people"]
        by_id = {x["id"]: x for x in everyone}
        if p == "/":
            body = (f"<p>{len(everyone)} people, newest hires first. Scroll to load more.</p><div id=list></div>"
                    "<p id=more class=muted>Loading…</p>" + """
<script>(function(){var off=0,busy=false,done=false,list=document.getElementById('list'),more=document.getElementById('more');
function load(){if(busy||done)return;busy=true;fetch('/people/api/list?offset='+off+'&limit=30').then(function(r){return r.json();})
.then(function(j){j.people.forEach(function(x){var c=document.createElement('div');c.className='card';
 c.innerHTML='<a href="/people/'+x.id+'"></a><br><span class=muted></span>';c.querySelector('a').textContent=x.name;
 c.querySelector('span').textContent=x.title+' · '+x.dept+' · '+x.office;list.appendChild(c);});
 off+=j.people.length;done=off>=j.total;more.textContent=done?'End of directory.':'Loading…';busy=false;
 if(!done&&more.getBoundingClientRect().top<innerHeight+200)load();});}
new IntersectionObserver(function(es){if(es[0].isIntersecting)load();},{rootMargin:'200px'}).observe(more);load();})();
</script>""")
            return self.send(200, page("Staff directory", body))
        if p == "/api/list":
            off, lim = max(0, as_int(q.get("offset"), 0)), min(50, max(1, as_int(q.get("limit"), 30)))
            rows = [{k: x[k] for k in ("id", "name", "title", "dept", "office")} for x in everyone[off:off + lim]]
            return self.json_reply(200, {"total": len(everyone), "people": rows})
        m = re.fullmatch(r"/(\d+)", p)
        x = by_id.get(int(m.group(1))) if m else None
        if not x:
            return self.not_found()
        return self.send(200, page(x["name"], self.profile(x, everyone, by_id)))

    def profile(self, x, everyone, by_id):
        import random
        rng = random.Random(f"profile:{STATE['seed']}:{x['id']}")

        def link(y):
            return f"<a href=/people/{y['id']}>{e(y['name'])}</a>"

        recent = rng.sample(everyone, 5)
        side = ("<div class=side><div class=card><b>Announcements</b><p>IT help desk: ext. 4000. Facilities: ext. 4100."
                "</p><p>Badge photos on Thursday in room 1.12.</p></div><div class=card><b>Recently viewed</b><ul>" +
                "".join(f"<li>{link(y)} · ext. {y['ext']}</li>" for y in recent) + "</ul></div></div>")
        boss = by_id.get(x["manager"])
        team = [y for y in everyone if y["dept"] == x["dept"] and y["id"] not in (x["id"], x["manager"])][:8]
        reports = [y for y in everyone if y["manager"] == x["id"]]
        card = (f"<div class=card><p>{e(x['title'])} · {e(x['dept'])} · {e(x['office'])} office</p><table>"
                f"<tr><th>Employee ID</th><td>{x['id']}</td></tr><tr><th>Desk ext.</th><td>{x['ext']}</td></tr>"
                f"<tr><th>Mobile</th><td>{x['mobile']}</td></tr><tr><th>Room</th><td>{x['room']}</td></tr>"
                f"<tr><th>Started</th><td>{short_date(x['hired'])}</td></tr>"
                f"<tr><th>Reports to</th><td>{link(boss) if boss else '—'}</td></tr></table></div>")
        rep = ("<h2>Direct reports</h2><ul>" + "".join(f"<li>{link(y)} · ext. {y['ext']}</li>" for y in reports[:12]) +
               "</ul>") if reports else ""
        peers = "<h2>Team</h2><ul>" + "".join(f"<li>{link(y)} · {e(y['title'])} · ext. {y['ext']}</li>" for y in team) + "</ul>"
        return f"<div class=cols><div class=grow>{card}{rep}{peers}</div>{side}</div>"

    # ------------------------------------------------------------------ cms

    def cms(self, p, q, w):
        posts = [STATE["posts"][str(x["id"])] for x in w["dw7"]["posts"]]
        if p == "/":
            items, page_no, pages = chunk(posts, as_int(q.get("p")), 10)
            rows = "".join(f"<tr><td><a href=/cms/posts/{x['id']}/edit>{e(x['title'])}</a></td><td>{e(x['category'])}</td>"
                           f"<td>{e(x['status'])}</td><td>{e(x['publish_at'].replace('T', ' ')) or '—'}</td>"
                           f"<td>{e(x['author'])}</td><td>{' '.join(e(t) for t in x['tags'])}</td></tr>" for x in items)
            saved = "<p role=status><b>Post saved.</b></p>" if q.get("saved") else ""
            body = (f"{saved}<p>{len(posts)} posts</p><table><tr><th>Title</th><th>Category</th><th>Status</th>"
                    f"<th>Publish at</th><th>Author</th><th>Tags</th></tr>{rows}</table>{pager('/cms/', q, page_no, pages)}")
            return self.send(200, page("Posts", body))
        m = re.fullmatch(r"/posts/(\d+)/edit", p)
        if m and m.group(1) in STATE["posts"]:
            return self.send(200, self.edit_form(STATE["posts"][m.group(1)], {}))
        return self.not_found()

    def edit_form(self, v, errs):
        summary = ""
        if errs:
            summary = ("<div class=errors role=alert><p><b>The post was not saved:</b></p><ul>" +
                       "".join(f"<li>{e(m)}</li>" for m in errs.values()) + "</ul></div>")
        chips = "".join(f"<span class=chip>{e(t)} <button type=button aria-label='Remove tag {e(t)}'>×</button>"
                        f"<input type=hidden name=tags value='{e(t)}'></span>" for t in v["tags"])
        opts = "".join(f"<li role=option id=cat-{i} data-value='{e(c)}' aria-selected=false>{e(c)}</li>"
                       for i, c in enumerate(td.CATEGORIES))
        statuses = "".join(f"<option{' selected' if s == v['status'] else ''}>{s}</option>"
                           for s in ("Draft", "Scheduled", "Published"))
        body = (f"{summary}<form method=post action=/cms/posts/{v['id']}/edit id=post>"
                f"<label>Title <input name=title size=60 value='{e(v['title'])}'></label>"
                f"<label>Slug <input name=slug size=60 value='{e(v['slug'])}'></label>"
                "<div style=position:relative><label for=catbox>Category</label>"
                f"<input id=catbox role=combobox aria-expanded=false aria-controls=catlist aria-autocomplete=list "
                f"autocomplete=off value='{e(v['category'])}'><input type=hidden name=category id=catval "
                f"value='{e(v['category'])}'><ul id=catlist role=listbox aria-label=Categories hidden>{opts}</ul></div>"
                f"<label>Publish at <input type=datetime-local name=publish_at value='{e(v['publish_at'])}'></label>"
                f"<label>Status <select name=status>{statuses}</select></label>"
                f"<fieldset><legend>Tags</legend><span id=chips>{chips}</span> <input id=newtag placeholder='Add a tag' "
                "aria-label='Add a tag'></fieldset><p><button>Save post</button> <a href=/cms/>Cancel</a></p></form>" + """
<script>(function(){
var box=document.getElementById('catbox'),val=document.getElementById('catval'),list=document.getElementById('catlist'),act=-1;
function opts(){return Array.from(list.children).filter(function(o){return !o.hidden;});}
function open(show){list.hidden=!show;box.setAttribute('aria-expanded',String(show));}
function mark(i){var o=opts();o.forEach(function(x,j){x.setAttribute('aria-selected',String(j===i));});act=i;
 if(o[i]){box.setAttribute('aria-activedescendant',o[i].id);o[i].scrollIntoView({block:'nearest'});}}
function pick(o){box.value=o.dataset.value;val.value=o.dataset.value;open(false);}
box.addEventListener('input',function(){var t=box.value.toLowerCase();Array.from(list.children).forEach(function(o){
 o.hidden=o.dataset.value.toLowerCase().indexOf(t)<0;});open(true);mark(-1);});
box.addEventListener('focus',function(){Array.from(list.children).forEach(function(o){o.hidden=false;});open(true);});
box.addEventListener('keydown',function(ev){var o=opts();if(ev.key==='ArrowDown'){mark(Math.min(act+1,o.length-1));ev.preventDefault();}
 else if(ev.key==='ArrowUp'){mark(Math.max(act-1,0));ev.preventDefault();}
 else if(ev.key==='Enter'){ev.preventDefault();if(o[act])pick(o[act]);}else if(ev.key==='Escape'){open(false);}});
list.addEventListener('mousedown',function(ev){var o=ev.target.closest('[role=option]');if(o){ev.preventDefault();pick(o);}});
box.addEventListener('blur',function(){setTimeout(function(){var hit=Array.from(list.children).find(function(o){
 return o.dataset.value.toLowerCase()===box.value.trim().toLowerCase();});if(hit)pick(hit);else box.value=val.value;open(false);},150);});
var chips=document.getElementById('chips'),nt=document.getElementById('newtag');
chips.addEventListener('click',function(ev){var b=ev.target.closest('button');if(b)b.parentNode.remove();});
nt.addEventListener('keydown',function(ev){if(ev.key!=='Enter')return;ev.preventDefault();var t=nt.value.trim().toLowerCase();
 if(!t)return;var s=document.createElement('span');s.className='chip';s.textContent=t+' ';var b=document.createElement('button');
 b.type='button';b.setAttribute('aria-label','Remove tag '+t);b.textContent='×';var h=document.createElement('input');h.type='hidden';
 h.name='tags';h.value=t;s.appendChild(b);s.appendChild(h);chips.appendChild(s);nt.value='';});
})();</script>""")
        return page(f"Edit post: {v['title']}", body)

    def cms_save(self, pid, f, w):
        cur = STATE["posts"].get(pid)
        if not cur:
            return self.not_found()
        tags = f.get("tags", [])
        tags = [tags] if isinstance(tags, str) else list(tags)
        v = dict(cur, title=str(f.get("title", "")).strip(), slug=str(f.get("slug", "")).strip(),
                 category=str(f.get("category", "")), status=str(f.get("status", "")),
                 publish_at=str(f.get("publish_at", ""))[:16], tags=[t.strip() for t in tags if t.strip()])
        errs = {}
        if not v["title"]:
            errs["title"] = "Title is required."
        if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", v["slug"]):
            errs["slug"] = "Slug may contain only lowercase letters, digits and dashes."
        if v["category"] not in td.CATEGORIES:
            errs["category"] = "Choose a category from the list."
        if v["status"] not in ("Draft", "Scheduled", "Published"):
            errs["status"] = "Choose a status."
        if v["publish_at"]:
            try:
                when = datetime.datetime.fromisoformat(v["publish_at"])
                if v["status"] == "Scheduled" and when.date() <= TODAY:
                    errs["publish_at"] = "A scheduled post needs a publish time in the future."
            except ValueError:
                errs["publish_at"] = "Enter a valid publish date and time."
        elif v["status"] == "Scheduled":
            errs["publish_at"] = "A scheduled post needs a publish date and time."
        if errs:
            return self.send(200, self.edit_form(v, errs))
        with LOCK:
            STATE["posts"][pid] = v
        return self.redirect(f"/cms/?saved={pid}")


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8794
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
