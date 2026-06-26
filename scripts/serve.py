#!/usr/bin/env python3
"""Build the WebCLAP bundle if needed, then serve the browser test host."""

from __future__ import annotations

import os
import runpy
import subprocess
import sys
from pathlib import Path


def main() -> None:
    port = sys.argv[1] if len(sys.argv) > 1 else "8000"
    root = Path(__file__).resolve().parents[1]
    web = root / "web"
    bundle = web / "parametric-hrtf.wclap.tar.gz"
    target_wasm = root / "target" / "wasm32-unknown-unknown" / "release" / "phrtf_webclap.wasm"

    if not bundle.is_file() or not target_wasm.is_file():
        print("Bundle or target wasm missing; building it first (cargo xtask bundle-webclap --release)...")
        subprocess.check_call(["cargo", "xtask", "bundle-webclap", "--release"], cwd=root)

    url = f"http://localhost:{port}/?module=parametric-hrtf.wclap.tar.gz&audio=audio/loop.mp3"
    print("------------------------------------------------------------")
    print(" Parametric HRTF - WebCLAP test host")
    print(f" Open: {url}")
    print(" Click once to start audio, then drag the source on the pad.")
    print("------------------------------------------------------------")

    os.chdir(web)
    sys.argv = [str(web / "server.py"), port]
    runpy.run_path(str(web / "server.py"), run_name="__main__")


if __name__ == "__main__":
    main()
