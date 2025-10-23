use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/styles.css");
    println!("cargo:rerun-if-changed=frontend/app.js");

    let template =
        fs::read_to_string("frontend/index.html").expect("failed to read frontend/index.html");
    let styles = fs::read_to_string("frontend/styles.css").expect("failed to read styles.css");
    let script = fs::read_to_string("frontend/app.js").expect("failed to read app.js");

    let bundled = template
        .replace("{{STYLE}}", styles.trim())
        .replace("{{SCRIPT}}", script.trim());

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    let out_path = out_dir.join("frontend_index.html");
    fs::write(&out_path, bundled).expect("failed to write bundled frontend");
}
