use std::path::Path;
use std::process::Command;

fn main() {
    let web = Path::new("web");

    for f in [
        "web/src",
        "web/package.json",
        "web/package-lock.json",
        "web/index.html",
        "web/vite.config.ts",
        "web/tsconfig.json",
    ] {
        println!("cargo:rerun-if-changed={f}");
    }
    println!("cargo:rerun-if-env-changed=SKIP_WEB_BUILD");

    if std::env::var_os("SKIP_WEB_BUILD").is_some() {
        ensure_dist_exists(web);
        return;
    }

    let npm = which("npm");
    if npm.is_none() {
        println!("cargo:warning=npm not found; using prebuilt web/dist (set SKIP_WEB_BUILD=1 to silence)");
        ensure_dist_exists(web);
        return;
    }
    let npm = npm.unwrap();

    if !web.join("node_modules").exists() {
        run(&npm, &["ci", "--silent"], web);
    }

    run(&npm, &["run", "build", "--silent"], web);
    ensure_dist_exists(web);
}

fn ensure_dist_exists(web: &Path) {
    let idx = web.join("dist").join("index.html");
    if !idx.exists() {
        panic!(
            "web/dist/index.html missing at {}. Run `npm --prefix {} run build` first.",
            idx.display(),
            web.display()
        );
    }
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(cmd);
        if full.is_file() {
            return Some(full);
        }
    }
    None
}

fn run(cmd: &Path, args: &[&str], cwd: &Path) {
    let status = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap_or_else(|e| panic!("run {} {:?}: {e}", cmd.display(), args));
    if !status.success() {
        panic!("{} {:?} failed: {status}", cmd.display(), args);
    }
}
