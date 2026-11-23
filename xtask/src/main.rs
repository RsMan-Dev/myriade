//! Repository tasks, run with `cargo run -p xtask -- TASK`.
//!
//! `package-server ENGINE OUTDIR` (also `package`): a server folder (docs/server.md): the server build of
//! the game (`cargo build --release -p horizon-game --no-default-features --bin horizon-client`) as
//! horizon-engine, with horizon-prepare and horizon-server from beside it, the exporters horizon-prepare
//! compiles and the sources of the release folders. It holds none of Minecraft's files.
//! First build the shared wasm standard library beside the programs with
//! `cargo run --locked -p cargo-horizon-mod -- wasm-std PROGRAM_FOLDER/horizon-wasm-std.wasm`.
//! The route requires and copies it; preflight never silently omits it.
//!
//! `mixins-tool release | patch SOURCE | identity TARGET`: the pinned horizon-mixins tool's
//! delivery contract. `release` prints its tag, pinned commit and source archive as `name=value`
//! lines; `patch` applies the carried patch to an unpacked source with the same applier as the
//! launcher's source build; `identity` prints the lines checked in a prebuilt's `identity.txt`.
//! The unsigned prebuilt-tool publishing workflow is deliberately not enabled.
//! `sdk-index DIST COMMIT DATE [--nightly NAME] [--previous BUILD DIFF]`: writes `DIST/sdk.json`,
//! the mod SDK's index, over the release assets in DIST (`tools/prebuilt_game.py index` calls it),
//! then reads it back and checks every asset against it. `--previous` names the SDK published
//! before this one and DIFF the `horizon-stage surface-diff --json` of its surface against this
//! one's, which fill the index's surface comparison.


use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: cargo run -p xtask -- package-server ENGINE OUTDIR | package ENGINE OUTDIR | mod-api [notes|coverage] | mixins-tool release | mixins-tool patch SOURCE | mixins-tool identity TARGET | sdk-index DIST COMMIT DATE [--nightly NAME] [--previous BUILD DIFF]";

/// What the release folder takes from the repository at HEAD, as `git archive` writes it.
/// `LICENSES/` holds the GPL text the LGPL in `LICENSE` builds on (LGPL-3.0 section 4b asks for
/// both beside the program) and the MIT notice of the code released before the change.
const ARCHIVED: [&str; 6] = [
    "tools",
    "mods/horizon/src/test/java",
    "crates/horizon-prepare",
    "research-lock.json",
    "LICENSE",
    "LICENSES",
];

// The programs beside the engine (horizon-prepare, horizon-server) and the wasm runtime are listed,
// with the checkout's exporters, in the SDK's required inputs (`horizon_mod_package::sdk::inputs`).

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [task, engine, out] if task == "package" || task == "package-server" => {
            package_server(Path::new(engine), Path::new(out))
        }
        [task, dist, commit, date, options @ ..] if task == "sdk-index" => {
            sdk_index(Path::new(dist), commit, date, options)
        }
        [task, what] if task == "mixins-tool" && what == "release" => {
            use horizon_mod_package::mixins_tool::{REV, release_tag};
            println!("tag={}\nrev={REV}", release_tag());
            println!("source={}/{REV}", horizon_addresses::HORIZON_MIXINS_SOURCE);
            Ok(())
        }
        [task, what, source] if task == "mixins-tool" && what == "patch" => {
            use horizon_mod_package::mixins_tool::{PATCH, PATCH_NAME, apply};
            apply(Path::new(source), PATCH)
                .map_err(|why| format!("{PATCH_NAME} does not apply to {source}: {why}"))
        }
        [task, what, target] if task == "mixins-tool" && what == "identity" => {
            print!("{}", horizon_mod_package::mixins_tool::identity(target));
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The checkout xtask acts on: the nearest folder at or above the current directory that holds
/// `xtask/Cargo.toml`. It is read when xtask runs, not when it is built, so a binary that
/// worktrees share through one `CARGO_TARGET_DIR` writes to the checkout it is run from.
fn repo_root() -> Result<PathBuf, String> {
    let here = std::env::current_dir().map_err(|e| format!("current directory: {e}"))?;
    checkout_above(&here)
        .ok_or_else(|| format!("{}: not inside a Horizon checkout", here.display()))
}

fn checkout_above(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join("xtask/Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn sdk_index(dist: &Path, commit: &str, date: &str, options: &[String]) -> Result<(), String> {
    use horizon_mod_package::sdk::{Build, GAME, INDEX, Index};
    let (mut nightly, mut previous) = (None, None);
    let mut rest = options;
    loop {
        rest = match rest {
            [] => break,
            [flag, name, more @ ..] if flag == "--nightly" => {
                nightly = Some(name.clone());
                more
            }
            [flag, build, diff, more @ ..] if flag == "--previous" => {
                previous = Some((build, Path::new(diff)));
                more
            }
            _ => return Err(USAGE.to_string()),
        };
    }
    let rust = fs::read_to_string(repo_root()?.join("tools/ci/rust-version"))
        .map_err(|e| format!("tools/ci/rust-version: {e}"))?;
    let build = Build {
        commit: commit.to_owned(),
        game: GAME.to_owned(),
        date: date.to_owned(),
        nightly,
    };
    let mut index =
        Index::of_folder(dist, build, rust.trim().to_owned()).map_err(|e| e.to_string())?;
    if let Some((build, diff)) = previous {
        let text = fs::read(diff).map_err(|e| format!("{}: {e}", diff.display()))?;
        index.compare(build, &text).map_err(|e| e.to_string())?;
    }
    let path = dist.join(INDEX);
    fs::write(&path, index.to_json()).map_err(|e| format!("{}: {e}", path.display()))?;
    let read = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Index::read(&read, commit)
        .and_then(|read| read.verify_folder(dist))
        .map_err(|e| e.to_string())?;
    println!(
        "wrote {} listing {} assets",
        path.display(),
        index.assets.len()
    );
    Ok(())
}

fn package_server(engine: &Path, out: &Path) -> Result<(), String> {
    preflight("package-server", engine)?;
    let repo = new_folder(out)?;
    copy_exporters(&repo.join("exporters"), &out.join("exporters"))?;
    copy(engine, &out.join(program_name(engine, "horizon-engine")))?;
    copy_runtime("package-server", engine, out)
}

/// Every required input of `route` (the SDK's one list) is there before anything is emptied,
/// built or copied; the refusal names all the missing ones.
fn preflight(route: &str, program: &Path) -> Result<(), String> {
    horizon_mod_package::sdk::inputs()
        .check(route, program, &repo_root()?)
        .map_err(|refusal| refusal.to_string())
}

/// Release builds supply only our compiled classes and their integrity manifest (a required
/// input, checked by `preflight`).
fn copy_exporters(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_exporters(&path, &target)?;
        } else {
            copy(&path, &target)?;
        }
    }
    Ok(())
}

/// `OUT`, emptied, with the sources the release folders take (`ARCHIVED`); the repository's path.
fn new_folder(out: &Path) -> Result<PathBuf, String> {
    let repo = repo_root()?;
    match fs::remove_dir_all(out) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("{}: {e}", out.display()));
        }
        _ => {}
    }
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut args = vec!["archive", "--format=tar", "HEAD"];
    args.extend(ARCHIVED);
    extract(&git(&repo, &args)?, out)?;
    Ok(repo)
}

/// `name`, with `.exe` when `program` has it.
fn program_name(program: &Path, name: &str) -> String {
    if program.extension().is_some_and(|e| e == "exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Copies the files `route` needs beside `program` (its companion programs and the wasm
/// runtime) into `out`.
fn copy_runtime(route: &str, program: &Path, out: &Path) -> Result<(), String> {
    let inputs = horizon_mod_package::sdk::inputs();
    let route = inputs.route(route).map_err(|e| e.to_string())?;
    for path in route.program_files(program) {
        copy_into(&path, out)?;
    }
    Ok(())
}

/// `cp FILE DIR/`.
fn copy_into(file: &Path, dir: &Path) -> Result<(), String> {
    let name = file
        .file_name()
        .ok_or_else(|| format!("{} is not a file", file.display()))?;
    copy(file, &dir.join(name))
}

/// `cp FROM TO` to a new file: the source's mode within the umask, so the programs stay
/// executable.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    let error = |e: std::io::Error| format!("{}: {e}", from.display());
    let data = fs::read(from).map_err(error)?;
    let permissions = fs::metadata(from).map_err(error)?.permissions();
    #[cfg(unix)]
    let mode = std::os::unix::fs::PermissionsExt::mode(&permissions);
    #[cfg(not(unix))]
    let mode = if permissions.readonly() { 0o444 } else { 0o666 };
    create(to, &data, mode)
}

/// A new file holding `data`, created with `mode` within the process umask as tar and cp create
/// theirs (on Windows only the read-only bit counts).
fn create(path: &Path, data: &[u8], mode: u32) -> Result<(), String> {
    use std::io::Write;
    let error = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode & 0o7777);
    options
        .open(path)
        .and_then(|mut file| file.write_all(data))
        .map_err(error)?;
    if cfg!(not(unix)) && mode & 0o222 == 0 {
        let mut permissions = fs::metadata(path).map_err(error)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).map_err(error)?;
    }
    Ok(())
}

/// A NUL-terminated tar header field.
fn field(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())]
}

fn octal(bytes: &[u8]) -> Result<u64, String> {
    let text = std::str::from_utf8(field(bytes)).map_err(|e| e.to_string())?;
    let text = text.trim_matches(|c: char| c == ' ' || c == '\0');
    u64::from_str_radix(if text.is_empty() { "0" } else { text }, 8)
        .map_err(|e| format!("bad tar number {text:?}: {e}"))
}

/// `path` under `out`, refusing anything that would land outside it.
fn inside(out: &Path, path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(path.trim_end_matches('/'));
    if !relative
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("unsafe path in the archive: {path}"));
    }
    Ok(out.join(relative))
}

/// The `path` record of a pax extended header, if it has one.
fn pax_path(mut records: &[u8]) -> Result<Option<String>, String> {
    let mut path = None;
    while !records.is_empty() {
        let space = records
            .iter()
            .position(|b| *b == b' ')
            .ok_or("bad pax record")?;
        let length: usize = std::str::from_utf8(&records[..space])
            .ok()
            .and_then(|n| n.parse().ok())
            .filter(|n| *n > space && *n <= records.len())
            .ok_or("bad pax record length")?;
        let record = &records[space + 1..length - 1];
        if let Some(value) = record.strip_prefix(b"path=") {
            path = Some(String::from_utf8(value.to_vec()).map_err(|e| e.to_string())?);
        }
        records = &records[length..];
    }
    Ok(path)
}

/// `tar -x -C out` for the ustar/pax archives `git archive` writes: directories and regular
/// files, created with the archive's modes within the process umask as tar creates them for an
/// ordinary user, so executables stay executable.
fn extract(tar: &[u8], out: &Path) -> Result<(), String> {
    let mut pos = 0;
    let mut long_path = None;
    while pos + 512 <= tar.len() {
        let header = &tar[pos..pos + 512];
        if header.iter().all(|b| *b == 0) {
            return Ok(());
        }
        let size = octal(&header[124..136])? as usize;
        let start = pos + 512;
        let data = tar
            .get(start..start + size)
            .ok_or("the archive ends inside an entry")?;
        pos = start + size.div_ceil(512) * 512;
        let path = match long_path.take() {
            Some(path) => path,
            None => {
                let name = String::from_utf8_lossy(field(&header[..100])).into_owned();
                let prefix = String::from_utf8_lossy(field(&header[345..500])).into_owned();
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                }
            }
        };
        let mode = octal(&header[100..108])? as u32;
        match header[156] {
            // The global header holds the commit id.
            b'g' => {}
            b'x' => long_path = pax_path(data)?,
            b'5' => {
                let dir = inside(out, &path)?;
                let mut builder = fs::DirBuilder::new();
                builder.recursive(true);
                #[cfg(unix)]
                std::os::unix::fs::DirBuilderExt::mode(&mut builder, mode & 0o7777);
                builder
                    .create(&dir)
                    .map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            b'0' | 0 => {
                let file = inside(out, &path)?;
                if let Some(parent) = file.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                create(&file, data, mode)?;
            }
            kind => {
                return Err(format!(
                    "{path}: unsupported entry type {:?} in the archive",
                    kind as char
                ));
            }
        }
    }
    Err("the archive has no end".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(name: &str, mode: u32, size: usize, kind: u8) -> Vec<u8> {
        let mut h = vec![0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(format!("{mode:07o}").as_bytes());
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[156] = kind;
        h[257..263].copy_from_slice(b"ustar\0");
        h
    }

    fn entry(tar: &mut Vec<u8>, name: &str, mode: u32, kind: u8, data: &[u8]) {
        tar.extend(header(name, mode, data.len(), kind));
        tar.extend(data);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }

    #[test]
    fn each_worktree_finds_its_own_checkout() {
        let base = std::env::temp_dir().join(format!("xtask-root-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let [one, two] = ["one", "two"].map(|name| base.join(name));
        for checkout in [&one, &two] {
            fs::create_dir_all(checkout.join("xtask")).unwrap();
            fs::write(checkout.join("xtask/Cargo.toml"), "").unwrap();
            fs::create_dir_all(checkout.join("crates/horizon-client/src")).unwrap();
        }
        assert_eq!(checkout_above(&one), Some(one.clone()));
        assert_eq!(
            checkout_above(&two.join("crates/horizon-client/src")),
            Some(two.clone())
        );
        assert_eq!(checkout_above(&base), None);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn archives_extract_with_their_paths_and_modes() {
        let out = std::env::temp_dir().join(format!("xtask-extract-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        let long = format!("tools/{}/run.sh", "d".repeat(120));
        let pax = format!("path={long}\n");
        let pax = format!("{} {pax}", pax.len() + 4);
        let mut tar = Vec::new();
        entry(
            &mut tar,
            "pax_global_header",
            0o666,
            b'g',
            b"52 comment=abc\n",
        );
        entry(&mut tar, "tools/", 0o775, b'5', b"");
        entry(&mut tar, "tools/a.py", 0o664, b'0', b"print(1)\n");
        entry(&mut tar, "tools/b.sh", 0o775, b'0', b"#!/bin/sh\n");
        entry(&mut tar, "PaxHeaders/x", 0o666, b'x', pax.as_bytes());
        entry(&mut tar, "ignored-name", 0o775, b'0', b"long\n");
        tar.extend([0; 1024]);
        extract(&tar, &out).unwrap();
        assert_eq!(fs::read(out.join("tools/a.py")).unwrap(), b"print(1)\n");
        assert_eq!(fs::read(out.join(&long)).unwrap(), b"long\n");
        assert!(!out.join("ignored-name").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &str| fs::metadata(out.join(p)).unwrap().permissions().mode();
            assert_eq!(mode("tools/a.py") & 0o111, 0);
            assert_eq!(mode("tools/b.sh") & 0o100, 0o100);
            assert_eq!(mode(&long) & 0o100, 0o100);
            assert_eq!(mode("tools") & 0o100, 0o100);
        }
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    #[ignore = "daily"]
    fn missing_inputs_are_named_before_the_folder_is_touched() {
        let base = std::env::temp_dir().join(format!("xtask-preflight-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let bin = base.join("bin");
        fs::create_dir_all(&bin).unwrap();
        for name in ["horizon-client", "horizon-prepare", "horizon-server"] {
            fs::write(bin.join(name), b"").unwrap();
        }
        let out = base.join("out");
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("kept"), b"").unwrap();
        let error = package_server(&bin.join("horizon-client"), &out).unwrap_err();
        assert!(error.contains("horizon-wasm-std.wasm"), "{error}");
        assert!(
            out.join("kept").is_file(),
            "the packager ran before the check"
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn archives_stay_inside_the_folder() {
        let out = Path::new("out");
        assert!(inside(out, "../x").is_err());
        assert!(inside(out, "/etc/x").is_err());
        assert_eq!(inside(out, "tools/").unwrap(), out.join("tools"));
        let mut tar = Vec::new();
        entry(&mut tar, "link", 0o777, b'2', b"");
        tar.extend([0; 1024]);
        assert!(extract(&tar, out).unwrap_err().contains("unsupported"));
    }
}
