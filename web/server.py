#!/usr/bin/env python3
import mimetypes
import posixpath
import sys
from http.server import HTTPServer, SimpleHTTPRequestHandler, test
from pathlib import Path
from urllib.parse import unquote, urlsplit

MIME_OVERRIDES = {
	'.js': 'text/javascript',
	'.mjs': 'text/javascript',
	'.wasm': 'application/wasm',
	'.json': 'application/json',
}

for ext, content_type in MIME_OVERRIDES.items():
	mimetypes.add_type(content_type, ext, strict=True)

ROOT = Path(__file__).resolve().parents[1]
TARGET_WASM_ROUTE = '/target/wasm32-unknown-unknown/release/phrtf_webclap.wasm'
TARGET_WASM_PATH = ROOT / 'target' / 'wasm32-unknown-unknown' / 'release' / 'phrtf_webclap.wasm'
UI_ASSET_ROUTES = {
	'/_phrtf-webclap-ui/index.html': ROOT / 'crates' / 'phrtf-webclap' / 'ui' / 'index.html',
	'/_phrtf-webclap-ui/main.js': ROOT / 'crates' / 'phrtf-webclap' / 'ui' / 'main.js',
	'/_phrtf-webclap-ui/protocol.js': ROOT / 'crates' / 'phrtf-webclap' / 'ui' / 'protocol.js',
	'/_phrtf-webclap-ui/styles.css': ROOT / 'crates' / 'phrtf-webclap' / 'ui' / 'styles.css',
}

class CustomRequestHandler (SimpleHTTPRequestHandler):
	extensions_map = SimpleHTTPRequestHandler.extensions_map.copy()
	extensions_map.update(MIME_OVERRIDES)

	def translate_path (self, path):
		clean_path = posixpath.normpath(unquote(urlsplit(path).path))
		if clean_path == TARGET_WASM_ROUTE:
			return str(TARGET_WASM_PATH)
		if clean_path in UI_ASSET_ROUTES:
			return str(UI_ASSET_ROUTES[clean_path])
		return SimpleHTTPRequestHandler.translate_path(self, path)

	def guess_type (self, path):
		clean_path = path.split('?', 1)[0].split('#', 1)[0]
		ext = posixpath.splitext(clean_path)[1].lower()
		if ext in MIME_OVERRIDES:
			return MIME_OVERRIDES[ext]
		return SimpleHTTPRequestHandler.guess_type(self, path)

	def end_headers (self):
		self.send_header('Access-Control-Allow-Origin', 'same-origin')
		self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
		self.send_header('Cross-Origin-Embedder-Policy', 'credentialless')
		self.send_header('Cache-Control', 'no-store')
		SimpleHTTPRequestHandler.end_headers(self)

port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
test(CustomRequestHandler, HTTPServer, port=port)
