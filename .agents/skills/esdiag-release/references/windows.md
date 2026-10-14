# Windows Release Target

Use this reference when building the native Windows release archive.

## Release Asset Contract

Attach this asset while the GitHub release is still a draft:

```text
esdiag-VERSION-x86_64-pc-windows-msvc.zip
```

The archive must contain exactly these root entries:

```text
esdiag.exe
LICENSE.txt
NOTICES.md
```

Add its lowercase SHA-256 record to `esdiag-VERSION-checksums.txt` with the
Homebrew archives.

## Build And Verify

Build on native Windows with the MSVC toolchain from tag-equivalent source.
The repository `.gitattributes` keeps embedded text assets LF-only, and
`.cargo/config.toml` statically links the C runtime. Do not set `RUSTFLAGS`,
which replaces the configured flags.

```powershell
git clone https://github.com/elastic/esdiag.git
cd esdiag
git checkout $TAG
git ls-files --eol | Select-String w/crlf   # must print nothing
$env:ESDIAG_GENERATE_NOTICE = "0"
cargo build --release --locked
target\release\esdiag.exe --version
```

From Windows PowerShell 5.1, run Cargo through `cmd /c` with redirected
output; PowerShell treats Cargo progress on stderr as a terminating error when
`$ErrorActionPreference` is `Stop`.

Verify that the executable imports only Windows system DLLs, with no
`VCRUNTIME140*.dll` or `api-ms-win-crt-*` entries:

```bash
objdump -p esdiag.exe | grep 'DLL Name'
```

Create the archive with `Compress-Archive`, verify its root entries, and
confirm `LICENSE.txt` and `NOTICES.md` are byte-identical to the other release
archives before uploading.
