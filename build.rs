fn main() {
    // The icon becomes resource 1 of both exes: Explorer and Task Manager show it, and
    // the tray loads it from there instead of from a file next to the program.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=clippath/clippath.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("clippath/clippath.ico");
        res.compile().expect("embed clippath.ico");
    }
}
