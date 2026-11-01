use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use brotli::enc::BrotliEncoderParams;

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("reads web build entry").path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn compress(data: &[u8]) -> Vec<u8> {
    let params = BrotliEncoderParams {
        quality: 11,
        size_hint: data.len(),
        ..Default::default()
    };
    let mut output = Vec::new();
    brotli::BrotliCompress(&mut &*data, &mut output, &params)
        .expect("compresses web file");
    output
}

fn get_content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

fn main() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/build/client");
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let target = out_dir.join("web");
    println!("cargo::rerun-if-changed={}", source.display());

    if target.exists() {
        fs::remove_dir_all(&target).expect("clears compressed web build");
    }
    fs::create_dir_all(&target).expect("creates compressed web build");

    let mut files = Vec::new();
    collect_files(&source, &mut files);
    let mut paths: Vec<String> = files
        .iter()
        .map(|file| {
            let relative =
                file.strip_prefix(&source).expect("file is inside the web build");
            relative.to_str().expect("web paths are UTF-8").replace('\\', "/")
        })
        .collect();

    let workers = thread::available_parallelism().map_or(4, usize::from);
    thread::scope(|scope| {
        for chunk in files.chunks(files.len().div_ceil(workers).max(1)) {
            let (source, target) = (&source, &target);
            scope.spawn(move || {
                for file in chunk {
                    let output = target.join(
                        file.strip_prefix(source).expect("file is inside the web build"),
                    );
                    fs::create_dir_all(output.parent().expect("file has a parent"))
                        .expect("creates directory");
                    fs::write(output, compress(&fs::read(file).expect("reads web file")))
                        .expect("writes compressed file");
                }
            });
        }
    });

    paths.sort();
    let mut table = String::from("&[\n");
    for path in &paths {
        let data = target.join(path);
        writeln!(
            table,
            "    WebFile {{ path: {path:?}, content_type: {:?}, data: include_bytes!({:?}) }},",
            get_content_type(path),
            data.to_str().expect("out dir is UTF-8"),
        )
        .expect("writes to string");
    }
    table.push(']');
    fs::write(out_dir.join("web.rs"), table).expect("writes web file table");
}
