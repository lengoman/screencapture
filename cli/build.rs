use std::process::Command;
use std::env;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    
    // We only want to rebuild the UI if something inside 'ui/' changes
    println!("cargo:rerun-if-changed=ui/src");
    println!("cargo:rerun-if-changed=ui/index.html");
    println!("cargo:rerun-if-changed=ui/package.json");
    println!("cargo:rerun-if-changed=ui/vite.config.ts");

    println!("Building React UI...");

    let npm_cmd = if cfg!(target_os = "windows") { "npm.cmd" } else { "npm" };

    // npm install
    let status = Command::new(npm_cmd)
        .current_dir("ui")
        .arg("install")
        .status()
        .expect("Failed to execute npm install. Is Node.js installed?");
        
    if !status.success() {
        panic!("npm install failed!");
    }

    // npm run build
    let status = Command::new(npm_cmd)
        .current_dir("ui")
        .arg("run")
        .arg("build")
        .status()
        .expect("Failed to execute npm run build. Is Node.js installed?");

    if !status.success() {
        panic!("npm run build failed!");
    }
}
