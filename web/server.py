#!/usr/bin/env python3
import sys
from http.server import HTTPServer, SimpleHTTPRequestHandler, test

class CustomRequestHandler (SimpleHTTPRequestHandler):
	extensions_map = SimpleHTTPRequestHandler.extensions_map.copy()
	extensions_map.update({
		'.js': 'text/javascript',
		'.mjs': 'text/javascript',
		'.wasm': 'application/wasm',
		'.json': 'application/json',
	})

	def end_headers (self):
		self.send_header('Access-Control-Allow-Origin', 'same-origin')
		self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
		self.send_header('Cross-Origin-Embedder-Policy', 'credentialless')
		SimpleHTTPRequestHandler.end_headers(self)

port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
test(CustomRequestHandler, HTTPServer, port=port)
