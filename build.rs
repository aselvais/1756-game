//! When the `bundle` feature is on, pack `images/` into OUT_DIR/images.zip.
//! Ruler portraits are left out: the Rust port does not load `images/rulers`.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_FEATURE_BUNDLE").is_err() {
        return;
    }
    println!("cargo:rerun-if-changed=images");
    if let Err(err) = bundle() {
        panic!("could not pack images for the executable: {err}");
    }
}

fn bundle() -> io::Result<()> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let images = Path::new(&manifest).join("images");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dest = Path::new(&out_dir).join("images.zip");
    let file = File::create(&dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    add_tree(&mut zip, &images, &images, opts)?;
    zip.finish()?;
    Ok(())
}

fn add_tree<W: Write + io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    root: &Path,
    dir: &Path,
    opts: zip::write::SimpleFileOptions,
) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "rulers" || name == ".DS_Store" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            add_tree(zip, root, &path, opts)?;
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel = rel.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(&path)?;
        zip.start_file(rel, opts)?;
        zip.write_all(&bytes)?;
    }
    Ok(())
}
