## patchall

```
patchall 0.1
Andrew Cann <shum@canndrew.org>
Patches all executable files in a directory for NixOS compatibility.

USAGE:
    patchall [FLAGS] [DIR]...

FLAGS:
    -d, --dry-run    Do a dry run. Don't actually patch anything, just print what actions would be performed.
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <DIR>...    Specify a directory.
```

# Synopsis

`patchall` recurses a set of directories looking for executable files that need
be fixed to work on NixOS. Specifically, these files include:

 * ELF executables whose dynamic loader does not exist. `patchall` sets the
   dynamic loader of these files to the dynamic loader currently used by the
   patchelf binary itself. It does this by calling NixOS `patchelf` utility
   which must be in your `$PATH`.
 * Shebang scripts with an interpreter other than `/bin/sh` or `/usr/bin/env`
   but whose interpreter is somewhere under `/{bin,sbin,usr,lib,lib64}`.
   `patchall` sets the shebang to `/usr/bin/env $INTERPRETER $ARGS` where
   $INTERPRETER is the original interpreter with the path removed and $ARGS is
   the original set of arguments. For instance
   `#!/sbin/somewhere/my-cool-tool foo bar` becomes
   `#!/usr/bin/env my-cool-tool foo bar`.

# Building from source

* [Install Rust](https://www.rust-lang.org/tools/install)
* Clone the repo
* `cd` into the repo
* `cargo build` to build
* `cargo install` to install

