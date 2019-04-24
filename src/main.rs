use {
    std::{
        io, fs, str,
        io::{Read, Write},
        path::{Path, PathBuf},
        process::Command,
        fs::File,
        ffi::OsStr,
        os::unix::fs::MetadataExt,
    },
    clap::{App, AppSettings, Arg, OsValues},
    walkdir::{DirEntry, WalkDir},
    unwrap::unwrap,
};

fn main() {
    let matches = {
        App::new("patchall")
        .author("Andrew Cann <shum@canndrew.org>")
        .about("Patches all executable files in a directory for NixOS compatibility.")
        .version("0.1")
        .setting(AppSettings::ArgRequiredElseHelp)
        .arg(
            Arg::with_name("DIR")
            .multiple(true)
            .help("Specify a directory.")
        )
        .arg(
            Arg::with_name("dry_run")
            .short("d")
            .long("dry-run")
            .help("Do a dry run. Don't actually patch anything, just print what actions would be performed.")
        )
        .get_matches()
    };
    let dirs = unwrap!(matches.values_of_os("DIR"));
    let dry_run = matches.is_present("dry_run");
    match run(dirs, dry_run) {
        Ok(()) => (),
        Err(e) => {
            eprintln!("{}", e);
        },
    }
}

fn run(dirs: OsValues, dry_run: bool) -> io::Result<()> {
    let self_path = fs::canonicalize("/proc/self/exe")?;
    let loader = get_loader(&self_path)?;
    let mut num_errors = 0;
    for dir in dirs {
        for entry_res in WalkDir::new(dir) {
            match try_patch_entry(entry_res, &loader, dry_run) {
                Ok(()) => (),
                Err(e) => {
                    num_errors += 1;
                    eprintln!("{}", e);
                },
            }
        }
    }
    if num_errors > 0 {
        println!("Finished. {} errors occured", num_errors);
    }
    Ok(())
}

fn try_patch_entry(entry_res: walkdir::Result<DirEntry>, loader: &Path, dry_run: bool) -> io::Result<()> {
    const MAGIC_BYTES_LEN: usize = 4;
    let entry = entry_res?;
    if !entry.file_type().is_file() {
        return Ok(());
    }
    let metadata = entry.metadata()?;
    let is_executable = metadata.mode() & 0o111 != 0;
    if !is_executable {
        return Ok(());
    }
    if metadata.len() < MAGIC_BYTES_LEN as u64 {
        return Ok(());
    }
    let mut file = File::open(entry.path())?;
    let mut magic_bytes = [0u8; MAGIC_BYTES_LEN];
    file.read_exact(&mut magic_bytes)?;
    drop(file);
    if &magic_bytes == b"\x74ELF" {
        return patch_elf(entry.path(), loader, dry_run);
    }
    if magic_bytes[..2] == b"#!"[..] {
        return patch_shebang(entry.path(), dry_run);
    }
    Ok(())
}

fn patch_elf(path: &Path, loader: &Path, dry_run: bool) -> io::Result<()> {
    let current_loader = get_loader(path)?;
    if current_loader.exists() {
        return Ok(());
    }
    println!("Patching {:?} to use {:?} instead of {:?}", path, loader, current_loader);
    if dry_run {
        return Ok(());
    }

    if !{
        Command::new("patchelf")
        .arg("--set-interpreter")
        .arg(loader)
        .arg(path)
        .status()?
        .success()
    } {
        return Err(io::Error::new(io::ErrorKind::Other, "patchelf failed"));
    }

    Ok(())
}

fn patch_shebang(path: &Path, dry_run: bool) -> io::Result<()> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    let newline_position = loop {
        let len = bytes.len();
        bytes.extend_from_slice(&[0u8; 256]);
        let n = file.read(&mut bytes[len..])?;
        bytes.truncate(len + n);
        if let Some(newline_position) = bytes.iter().position(|b| *b == b'\n') {
            break newline_position;
        }
        if n == 0 {
            return Ok(());
        }
    };
    let shebang = match str::from_utf8(&bytes[..newline_position]) {
        Ok(shebang) => shebang,
        Err(_) => return Ok(()),
    };
    let mut chunks = shebang[2..].trim().split_whitespace();
    let interpreter = match chunks.next() {
        Some(interpreter) => interpreter,
        None => return Ok(()),
    };
    if interpreter == "/bin/sh" {
        return Ok(());
    }
    if interpreter == "/usr/bin/env" {
        return Ok(());
    }
    if !{
        ["/bin", "/lib", "/lib64", "/sbin", "/usr"]
        .iter()
        .any(|prefix| interpreter.starts_with(prefix))
    } {
        return Ok(());
    }

    let interpreter_executable = match Path::new(interpreter).file_name() {
        Some(interpreter_executable) => unwrap!(interpreter_executable.to_str()),
        None => return Ok(()),
    };

    println!("Patching shebang of {:?} to /usr/bin/env {}", path, interpreter_executable);
    if dry_run {
        return Ok(());
    }

    let mut contents = bytes[newline_position..].to_owned();
    file.read_to_end(&mut contents)?;

    let tmp_file_name = format!("/tmp/patchall-{:08x}", rand::random::<u32>());
    let mut tmp_file = File::create(&tmp_file_name)?;
    write!(tmp_file, "#!/usr/bin/env {}", interpreter_executable)?;
    for arg in chunks {
        write!(tmp_file, " {}", arg)?;
    }

    tmp_file.write_all(&contents)?;
    drop(file);
    drop(tmp_file);

    fs::rename(tmp_file_name, path)?;

    Ok(())
}

fn get_loader(path: &Path) -> io::Result<PathBuf> {
    let ldd_output = Command::new("ldd").arg(path).output()?;
    if !ldd_output.status.success() {
        let err = String::from_utf8_lossy(&ldd_output.stderr);
        return Err(io::Error::new(io::ErrorKind::Other, err));
    }
    let output = String::from_utf8_lossy(&ldd_output.stdout);
    for line in output.lines() {
        let lib_end = match line.find("=>") {
            Some(lib_end) => lib_end,
            None => continue,
        };
        let lib = line[..lib_end].trim();
        let path = Path::new(lib);
        if path.file_name() != Some(OsStr::new("ld-linux-x86-64.so.2")) {
            continue;
        }
        return Ok(path.to_owned());
    }
    Err(io::Error::new(io::ErrorKind::Other, "\
        unable to determine path to dynamic loader. Note: patchall must be dynamically linked for \
        it to work since it checks its own dynamic loader to determine the path to the dynamic \
        loader.\
    "))
}

