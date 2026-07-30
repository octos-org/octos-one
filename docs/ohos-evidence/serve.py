#!/usr/bin/env python3
"""Static server that is explicit about UTF-8.

`python3 -m http.server` sends `Content-Type: text/javascript` with no charset.
The octos web kit contains emoji character-class regexes, and a Chinese-locale
device falls back to GBK, corrupting them into
"Invalid regular expression: ... Range out of order in character class" — which
kills octos.core before it defines anything.
"""
import http.server
import socketserver

PORT = 8731


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".js": "application/javascript; charset=utf-8",
        ".html": "text/html; charset=utf-8",
        ".json": "application/json; charset=utf-8",
        "": "application/octet-stream",
    }

    def end_headers(self):
        # The device browser caches aggressively; this is a dev loop.
        self.send_header("Cache-Control", "no-store, must-revalidate")
        super().end_headers()

    def log_message(self, fmt, *args):
        super().log_message(fmt, *args)


socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(("0.0.0.0", PORT), Handler) as httpd:
    print(f"serving {PORT} with charset=utf-8", flush=True)
    httpd.serve_forever()
