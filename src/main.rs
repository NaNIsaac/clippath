//! `clippath.exe`, the command line.

use std::process::ExitCode;

const USAGE: &str = "\
clippath: Screenshot once. Paste where you need it.

usage: clippath <command> [--wsl | --windows]

  start    watch in the background and at every login
  stop     stop the background watcher and remove it from login
  status   show watcher, autostart and saved files
  watch    watch in the foreground (Ctrl+C to stop)
  copy     convert the clipboard image once
  clean    delete saved screenshots (--days N keeps the last N days)

--wsl copies /mnt/c/... paths, --windows C:\\... paths (the default).
Quick start: run `clippath start`, then screenshot and Ctrl+V in your terminal.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("{message}");
            ExitCode::from(code)
        }
    }
}

fn run(args: &[&str]) -> Result<(), (u8, String)> {
    let usage = || (2, USAGE.to_string());
    let paths = |rest: &[&str]| -> Result<bool, (u8, String)> {
        match rest {
            [] | ["--windows"] => Ok(false),
            ["--wsl"] => Ok(true),
            _ => Err(usage()),
        }
    };
    let failed = |e: String| (1, format!("clippath: {e}"));
    match args {
        ["-V" | "--version"] => println!("clippath {}", env!("CARGO_PKG_VERSION")),
        [] | ["-h" | "--help"] => println!("{USAGE}"),
        #[cfg(windows)]
        ["copy", rest @ ..] => {
            let c = clippath::capture::capture(paths(rest)?, std::ptr::null_mut()).map_err(failed)?;
            let c = c.ok_or_else(|| failed("no image on clipboard".into()))?;
            println!("File:   {}\nCopied: {}", c.path.display(), c.out);
        }
        #[cfg(windows)]
        ["watch", rest @ ..] => {
            clippath::watch::run(paths(rest)?, Box::new(|line| println!("{line}"))).map_err(failed)?;
        }
        #[cfg(windows)]
        ["start", rest @ ..] => println!("{}", clippath::service::start(paths(rest)?).map_err(failed)?),
        #[cfg(windows)]
        ["stop"] => {
            clippath::service::stop();
            println!("Stopped. Autostart off.");
        }
        #[cfg(windows)]
        ["status"] => println!("{}", clippath::service::status().map_err(failed)?),
        #[cfg(windows)]
        ["clean", rest @ ..] => {
            let days = match rest {
                [] => 0.0,
                ["--days", n] => n.parse().map_err(|_| usage())?,
                _ => return Err(usage()),
            };
            println!("Removed {} file(s).", clippath::service::clean(days).map_err(failed)?);
        }
        #[cfg(not(windows))]
        [_, ..] => return Err((1, "clippath: Windows only".into())),
        _ => return Err(usage()),
    }
    Ok(())
}
