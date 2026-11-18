"""A minimal Chrome DevTools Protocol client on the standard library: one WebSocket (RFC 6455, client side, masked
frames, fragments, ping/pong) to one page target, plus the few page helpers the dev solver needs. Not tabref.
"""
import base64
import json
import os
import shutil
import socket
import struct
import subprocess
import time
import urllib.request
from urllib.parse import urlparse

CHROME_FLAGS = ["--headless=new", "--no-first-run", "--no-default-browser-check", "--disable-sync",
                "--password-store=basic", "--disable-extensions", "--window-size=1280,900"]


class CDPError(Exception):
    pass


class WebSocket:
    def __init__(self, url, timeout=60):
        u = urlparse(url)
        self.sock = socket.create_connection((u.hostname, u.port), timeout=timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        req = (f"GET {u.path} HTTP/1.1\r\nHost: {u.hostname}:{u.port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
               f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n")
        self.sock.sendall(req.encode())
        head = b""
        while b"\r\n\r\n" not in head:
            chunk = self.sock.recv(1)
            if not chunk:
                raise CDPError("websocket handshake closed")
            head += chunk
        if b" 101 " not in head.split(b"\r\n", 1)[0]:
            raise CDPError(f"websocket handshake refused: {head[:80]!r}")

    def _exact(self, n):
        out = b""
        while len(out) < n:
            chunk = self.sock.recv(n - len(out))
            if not chunk:
                raise CDPError("websocket closed")
            out += chunk
        return out

    def _frame(self, opcode, payload):
        head = bytes([0x80 | opcode])
        n = len(payload)
        if n < 126:
            head += bytes([0x80 | n])
        elif n < 65536:
            head += bytes([0x80 | 126]) + struct.pack(">H", n)
        else:
            head += bytes([0x80 | 127]) + struct.pack(">Q", n)
        mask = os.urandom(4)
        self.sock.sendall(head + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(payload)))

    def send(self, text):
        self._frame(0x1, text.encode())

    def recv(self):
        parts = []
        while True:
            b0, b1 = self._exact(2)
            opcode, n = b0 & 0x0F, b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._exact(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._exact(8))[0]
            mask = self._exact(4) if b1 & 0x80 else None
            data = self._exact(n)
            if mask:
                data = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
            if opcode == 0x9:
                self._frame(0xA, data)
                continue
            if opcode == 0x8:
                raise CDPError("websocket closed by Chrome")
            if opcode in (0x0, 0x1, 0x2):
                parts.append(data)
                if b0 & 0x80:
                    return b"".join(parts).decode()

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


class Page:
    """One page target. js() evaluates an expression (promises awaited) and returns its value."""

    def __init__(self, ws_url):
        self.ws = WebSocket(ws_url)
        self.next_id = 0
        self.call("Page.enable")
        self.call("Runtime.enable")

    def call(self, method, **params):
        self.next_id += 1
        mid = self.next_id
        self.ws.send(json.dumps({"id": mid, "method": method, "params": params}))
        while True:
            msg = json.loads(self.ws.recv())
            if msg.get("id") == mid:
                if "error" in msg:
                    raise CDPError(f"{method}: {msg['error']}")
                return msg.get("result", {})

    def js(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True, awaitPromise=True)
        if r.get("exceptionDetails"):
            raise CDPError(f"js failed: {r['exceptionDetails'].get('exception', {}).get('description', r)}"[:400])
        return r.get("result", {}).get("value")

    def wait(self, expr, timeout=20, what=None):
        end = time.time() + timeout
        while time.time() < end:
            try:
                v = self.js(f"!!({expr})")
            except CDPError:
                v = None
            if v:
                return v
            time.sleep(0.15)
        raise CDPError(f"timed out waiting for {what or expr}")

    def goto(self, url, timeout=45):
        self.call("Page.navigate", url=url)
        self.settle(timeout)

    def mark(self):
        self.js("window.__solverMark = 1")

    def settle(self, timeout=45):
        """Wait for a new document (mark() before the action) that has finished loading."""
        self.wait("window.__solverMark === undefined && document.readyState === 'complete'", timeout, "page load")

    def point(self, el_expr):
        """Viewport centre of an element (through same-origin iframes), after scrolling it into view."""
        return self.js(f"""(function(){{var el={el_expr};if(!el)return null;el.scrollIntoView({{block:'center'}});
var r=el.getBoundingClientRect(),x=r.left+r.width/2,y=r.top+r.height/2,w=el.ownerDocument.defaultView;
while(w.frameElement){{var f=w.frameElement,fr=f.getBoundingClientRect();x+=fr.left+f.clientLeft;y+=fr.top+f.clientTop;w=w.parent;}}
return [x,y];}})()""")

    def click(self, el_expr):
        pt = self.point(el_expr)
        if not pt:
            raise CDPError(f"nothing to click: {el_expr}")
        x, y = pt
        self.call("Input.dispatchMouseEvent", type="mouseMoved", x=x, y=y)
        self.call("Input.dispatchMouseEvent", type="mousePressed", x=x, y=y, button="left", clickCount=1)
        self.call("Input.dispatchMouseEvent", type="mouseReleased", x=x, y=y, button="left", clickCount=1)

    def type_into(self, el_expr, text, clear=True):
        """Focus the element, select what is there, and type over it as keyboard input."""
        self.click(el_expr)
        if clear:
            self.js(f"(function(){{var el={el_expr};el.focus();if(el.select)el.select();}})()")
            self.call("Input.dispatchKeyEvent", type="keyDown", key="Backspace", code="Backspace",
                      windowsVirtualKeyCode=8)
            self.call("Input.dispatchKeyEvent", type="keyUp", key="Backspace", code="Backspace",
                      windowsVirtualKeyCode=8)
        self.call("Input.insertText", text=text)

    def set_value(self, el_expr, value):
        """What an agent does for date, time and select inputs: set the value, fire input and change."""
        self.js(f"""(function(){{var el={el_expr};el.focus();
var proto=el.tagName==='SELECT'?HTMLSelectElement.prototype:el.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;
Object.getOwnPropertyDescriptor(proto,'value').set.call(el,{json.dumps(value)});
el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));
return el.value;}})()""")

    def close(self):
        self.ws.close()


class Chrome:
    """A headless Chrome on a fresh profile with a DevTools port; stop() kills it and removes the profile."""

    def __init__(self, profile, chrome="/usr/bin/google-chrome"):
        self.profile = profile
        shutil.rmtree(profile, ignore_errors=True)
        os.makedirs(profile)
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            self.port = s.getsockname()[1]
        self.proc = subprocess.Popen([chrome, *CHROME_FLAGS, f"--user-data-dir={profile}",
                                      f"--remote-debugging-port={self.port}", "about:blank"],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        end = time.time() + 30
        while time.time() < end:
            try:
                self.targets()
                return
            except OSError:
                time.sleep(0.2)
        self.stop()
        raise CDPError("Chrome did not open its DevTools port")

    def targets(self):
        with urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/list", timeout=5) as r:
            return json.loads(r.read())

    def new_page(self):
        req = urllib.request.Request(f"http://127.0.0.1:{self.port}/json/new?about:blank", method="PUT")
        with urllib.request.urlopen(req, timeout=10) as r:
            t = json.loads(r.read())
        return Page(t["webSocketDebuggerUrl"]), t["id"]

    def close_page(self, target_id):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/close/{target_id}", timeout=5).read()
        except OSError:
            pass

    def stop(self):
        try:
            os.killpg(self.proc.pid, 9)
        except OSError:
            pass
        self.proc.wait(timeout=10)
        shutil.rmtree(self.profile, ignore_errors=True)
