//! `cargo xtask bundle-webclap [--release]`
//!
//! Builds the `phrtf-webclap` crate for `wasm32-unknown-unknown` and assembles
//! a real WCLAP bundle:
//!
//! ```text
//! dist/parametric-hrtf.wclap/
//!     module.wasm        (the compiled plugin)
//!     plugin.json        (manifest, artifact rewritten for the tarball)
//!     ui/                (index.html, main.js, styles.css)
//! dist/parametric-hrtf.wclap.tar.gz   (same, archived — what hosts load)
//! web/parametric-hrtf.wclap.tar.gz    (copy served by the bundled host)
//! ```
//!
//! Modelled on z-audio-dsp-plugin's `xtask bundle-webclap`, trimmed to this
//! one plugin and with no nih-plug dependency.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::write::GzEncoder;
use flate2::Compression;

const PACKAGE: &str = "phrtf-webclap";
const WASM_FILE: &str = "phrtf_webclap.wasm";
const BUNDLE_NAME: &str = "parametric-hrtf.wclap";
const CRATE_DIR: &str = "crates/phrtf-webclap";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bundle-webclap") => bundle_webclap(&args[1..]),
        other => {
            eprintln!("unknown task {other:?}; usage: cargo xtask bundle-webclap [--release]");
            std::process::exit(2);
        }
    }
}

fn bundle_webclap(args: &[String]) -> Result<()> {
    let root = workspace_root();
    let release = args.iter().any(|a| a == "--release");
    let profile = if release { "release" } else { "debug" };

    // 1. Build the plugin to wasm.
    let mut build = Command::new(env_cargo());
    build
        .current_dir(&root)
        .arg("build")
        .arg("-p")
        .arg(PACKAGE)
        .arg("--target")
        .arg("wasm32-unknown-unknown");
    if release {
        build.arg("--release");
    }
    let status = build.status()?;
    if !status.success() {
        return Err("cargo build for wasm32-unknown-unknown failed".into());
    }

    let wasm_path = root
        .join("target/wasm32-unknown-unknown")
        .join(profile)
        .join(WASM_FILE);
    if !wasm_path.is_file() {
        return Err(format!("expected wasm at {}", wasm_path.display()).into());
    }

    // 2. Assemble the .wclap directory.
    let dist = root.join("dist");
    fs::create_dir_all(&dist)?;
    let bundle_dir = dist.join(BUNDLE_NAME);
    if bundle_dir.exists() {
        fs::remove_dir_all(&bundle_dir)?;
    }
    fs::create_dir_all(&bundle_dir)?;

    fs::copy(&wasm_path, bundle_dir.join("module.wasm"))?;

    let crate_dir = root.join(CRATE_DIR);
    let archive_name = format!("{BUNDLE_NAME}.tar.gz");
    let manifest = fs::read_to_string(crate_dir.join("plugin.json"))?;
    let manifest = rewrite_manifest(&manifest, &archive_name);
    fs::write(bundle_dir.join("plugin.json"), manifest)?;

    let ui_src = crate_dir.join("ui");
    if ui_src.is_dir() {
        copy_dir_recursive(&ui_src, &bundle_dir.join("ui"))?;
    }

    // 3. Archive it (this is what WCLAP hosts fetch).
    let archive_path = dist.join(&archive_name);
    create_archive(&bundle_dir, &archive_path)?;

    // 4. Drop a copy next to the bundled browser host so it can be served
    //    same-origin without any extra wiring.
    let web_copy = root.join("web").join(&archive_name);
    if web_copy.parent().map(|p| p.is_dir()).unwrap_or(false) {
        fs::copy(&archive_path, &web_copy)?;
        eprintln!("Copied bundle to {}", web_copy.display());
    }

    eprintln!("Created WebCLAP bundle  {}", bundle_dir.display());
    eprintln!("Created WebCLAP tarball {}", archive_path.display());
    Ok(())
}

/// Rewrite the manifest's `artifact`/`format` so the published manifest points
/// at the tarball rather than the build-tree wasm (mirrors the upstream xtask).
fn rewrite_manifest(manifest: &str, archive_name: &str) -> String {
    let mut out: String = manifest
        .lines()
        .map(|line| {
            let t = line.trim_start();
            if t.starts_with("\"artifact\"") {
                format!("  \"artifact\": \"{archive_name}\",")
            } else if t.starts_with("\"format\"") {
                "  \"format\": \"tar.gz\",".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

fn create_archive(bundle_dir: &Path, archive_path: &Path) -> Result<()> {
    if archive_path.exists() {
        fs::remove_file(archive_path)?;
    }
    let file = fs::File::create(archive_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut archive = tar::Builder::new(encoder);
    archive.append_path_with_name(bundle_dir.join("module.wasm"), "module.wasm")?;
    archive.append_path_with_name(bundle_dir.join("plugin.json"), "plugin.json")?;
    let ui_dir = bundle_dir.join("ui");
    if ui_dir.is_dir() {
        archive.append_dir_all("ui", ui_dir)?;
    }
    archive.into_inner()?.finish()?;
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        // Don't ship test files in the plugin bundle.
        if name.to_string_lossy().contains(".test.") {
            continue;
        }
        let dest = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

fn env_cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// The xtask is launched from the workspace root by `cargo xtask`; resolve it
/// from the manifest dir (crates/xtask) up two levels as a fallback.
fn workspace_root() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_WORKSPACE_DIR") {
        return PathBuf::from(dir);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}
