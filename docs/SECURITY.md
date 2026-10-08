# The AllInsight safety model

This document describes what stops AllInsight from deleting something it should
not. It is written to be checked against the code rather than believed.

The order of priorities throughout is: **safety, then privacy, then
correctness, then performance, then features.** Where they conflict, the
earlier one wins, and several features in this application are narrower than
they could be for exactly that reason.

---

## 1. The frontend cannot name a file

The interface never sends a path to a destructive command. Look at the
signature of the only command that removes cleanup candidates:

```rust
pub struct CleanupRequest {
    pub scan_id: u64,
    pub categories: Vec<CleanupCategory>,   // an enum variant
    pub candidate_ids: Vec<String>,         // ids this backend issued
    pub confirmed: bool,
}
```

There is no `Vec<PathBuf>` here, and there is no variant of this type that
carries one. A candidate id is `{scan_id}:{index}` into a table the backend
holds in memory. An id from a previous scan is rejected outright:

```rust
if request.scan_id != scan.scan_id {
    return Err(AllInsightError::InvalidInput(
        "This cleanup preview is out of date. Run the scan again.".into(),
    ));
}
```

This matters because it makes a whole class of bug impossible rather than
merely unlikely. If the signature took paths, then every caller would have to
validate them correctly forever. It does not, so they cannot get it wrong.

The two review screens, Large Files and Duplicates, do accept a path, because
they deal in files the user chose rather than in categories. That path is
checked against the results the backend itself produced before anything
happens:

```rust
if !known {
    return Err(AllInsightError::UnknownCandidate(
        "That file was not part of the last scan. Run the scan again.".into(),
    ));
}
```

and removal there always goes to the Recycle Bin, never to permanent deletion.

---

## 2. Removal requires a token that cannot be forged

```rust
pub struct ValidatedPath {
    path: PathBuf,      // private
    kind: EntryKind,    // private
    size_bytes: u64,    // private
}
```

The fields are private and there is no public constructor. The only way to
obtain a `ValidatedPath` is `DeletionGuard::validate`. Every removal function
takes one:

```rust
pub fn remove(entry: &ValidatedPath, mode: DeletionMode) -> Result<()>
```

There is deliberately no function in `services::cleanup::remove` that accepts
a `&Path`. Possession of the token *is* the proof that the checks ran.

---

## 3. What the guard checks, in order

Each step ends the sequence on failure.

1. **Lexical normalisation.** `..` and `.` are resolved without touching the
   disk, so `C:\Windows\Temp\..\..\Users\Me\Documents` is collapsed to its
   real target *before* it is compared with anything.
2. **The protected-path engine.** See section 4.
3. **Allow-list membership.** The path must be strictly inside one of the
   directories the requesting cleanup category declared. Cleanup is opt-in by
   location: a path that no category claims is refused even if nothing else
   objects to it.
4. **Existence.** Via `symlink_metadata`, which does not follow links.
5. **The entry is not a reparse point.** Symlinks, junctions and mount points
   are refused, never followed.
6. **No directory between the allowed root and the entry is a reparse point.**
   A junction planted inside a temp folder pointing at `Documents` is the
   attack this closes.
7. **Everything above, again, on the canonicalised path.** This step is the
   authoritative one; the rest is defence in depth.
8. **The entry is still the kind of thing the scan recorded.** A file that
   became a directory between preview and confirmation is skipped.

Steps 4 through 8 run at the moment of deletion, not when the preview was
generated. The filesystem does not hold still, and a verdict recorded minutes
ago is not evidence about now.

---

## 4. The protected-path engine

`ProtectedPaths` answers "may AllInsight touch this?" It is built at startup from
Windows known folders rather than from hard-coded strings, because Documents
and Desktop are frequently redirected to OneDrive or a second drive, and a
hard-coded `C:\Users\x\Documents` would quietly miss the real one.

**Protected roots** include `%SystemRoot%`, both Program Files directories,
`%ProgramData%`, per-drive boot and recovery data, System Volume Information,
`$Recycle.Bin`, Documents, Desktop, Pictures, Videos, Music, Downloads,
OneDrive, `.ssh`, `.gnupg`, `.aws`, browser profiles, and anything the user
adds in Settings.

**Protected by name or extension, anywhere:** databases (`.db`, `.sqlite`,
`.mdf`, ...), key stores (`.kdbx`, `.pem`, `.pfx`, ...), virtual machine disks,
backups, and the specific files browsers keep credentials in.

**Protected by containing directory:** anything under `.git`, `.svn`, `.hg`,
`.ssh`, `.gnupg`.

### Ancestry is component-wise, never `starts_with`

```rust
assert!(!is_within(Path::new("C:\\Users\\Bobby"), Path::new("C:\\Users\\Bob")));
```

String prefix matching would call `C:\Users\Bobby` a child of `C:\Users\Bob`.
Comparison is done on the sequence of lower-cased path components, so it
cannot. Case-insensitivity is real (NTFS is), and the `\\?\` verbatim prefix is
stripped before comparison so a canonicalised path and a typed one compare
equal.

---

## 5. Carve-outs: the one exception, and its limits

`C:\Windows` is protected, but `C:\Windows\Temp` exists precisely to hold
disposable files. Refusing to clean it would make the product useless for its
main job, so there is a narrow exception mechanism.

A refusal can be lifted only when **all** of these hold:

- the reason is an enclosing-root reason (`WindowsDirectory`, `ProgramFiles`,
  `ProgramData`, `BrowserProfile`) - never a filename, extension, drive-root,
  hostile-name, or user-configured reason;
- the path sits inside one of the requesting category's declared roots;
- **and** that root appears in `CARVE_OUT_TEMPLATES`, a compile-time constant.

The whole list:

```
%SystemRoot%\Temp
%SystemRoot%\SoftwareDistribution\Download
%SystemRoot%\Logs\CBS
%ProgramData%\Microsoft\Windows\WER\ReportArchive
%ProgramData%\Microsoft\Windows\WER\ReportQueue
%ProgramData%\Microsoft\Windows\DeliveryOptimization\Cache
%LOCALAPPDATA%\<browser>\User Data\*\Cache\Cache_Data
%LOCALAPPDATA%\<browser>\User Data\*\Code Cache
%LOCALAPPDATA%\<browser>\User Data\*\GPUCache
```

Both halves of the intersection are compiled in. Nothing in settings, in the
IPC surface, or in a model's output can widen it. **A folder the user added to
the protected list is never lifted, whatever else is true** - `UserConfigured`
is deliberately absent from the liftable set, and there is a test that says so.

The `*` matches exactly one path component, which is how Chromium's `Default`,
`Profile 1`, `Profile 2` are covered without enumerating them.

There is a second, much narrower exception for filenames: Windows names its
thumbnail cache `thumbcache_*.db`, which the blanket `.db` rule would refuse.
A category may declare a filename prefix exemption, and it lifts only the
`Database` reason and only for names matching that prefix. It cannot reach a
protected root:

```rust
#[test]
fn a_name_exemption_cannot_reach_a_protected_root() { /* ... */ }
```

---

## 6. Cleanup categories are types, not strings

```rust
pub enum CleanupCategory {
    WindowsTemp, UserTemp, CrashDumps, WindowsErrorReporting,
    ThumbnailCache, IconCache, ShaderCache, BrowserCache,
    WindowsUpdateCache, DeliveryOptimizationCache,
    ComponentStoreLogs, FontCache, RecycleBin,
}
```

Each variant has one compile-time definition: a fixed set of directories, a
fixed matching rule (all contents, an extension list, or a filename prefix
list), a deletion mode, a minimum age, and the two sentences shown in the
confirmation dialog. There is no code path that turns a string into a
deletion.

A test asserts the central property directly:

```rust
#[test]
fn no_category_root_reaches_protected_storage() {
    for d in definitions() {
        for root in &d.roots {
            if protected.classify(root).protected {
                assert!(protected.is_carve_out(root));
            }
        }
    }
}
```

---

## 7. Auto-Clean is narrower still

Automatic cleanup runs unattended, so its eligible set is derived from the
definitions rather than configured, and the user's selection is *intersected*
with it:

```rust
pub fn effective_auto_clean_categories(&self) -> Vec<CleanupCategory> {
    auto_clean_categories()                      // compiled-in eligible set
        .into_iter()
        .filter(|c| self.auto_clean_categories.contains(&name_of(c)))
        .collect()
}
```

Naming `recycle_bin` or `windows_update_cache` in the settings database does
not enable them. The Recycle Bin is never automatic because emptying it is not
reversible; browser caches are not automatic because clearing one under a
running browser makes it immediately rewrite the cache; elevated categories are
not automatic because a background process should not be making decisions about
a half-finished Windows update.

---

## 8. Reversibility

Removal defaults to the Recycle Bin. Permanent deletion is reserved for caches
that Windows or the owning application regenerates on demand, where a Recycle
Bin copy would occupy exactly the space the cleanup was meant to reclaim.

Directories are removed with `remove_dir`, never `remove_dir_all`, so a mistake
in a matching rule cannot take a populated folder with it. An entry that
vanished before removal is a success, not an error: the desired end state was
reached.

---

## 9. The local model cannot act

The model receives a text briefing built from measurements. It returns a
string. That string is displayed and nothing else happens to it:

- no part of AllInsight parses model output for commands, paths or identifiers;
- the buttons under an answer come from `actions_for(facts)`, which reads
  measurements, not text;
- output is stripped of control characters and Unicode bidirectional
  overrides, then capped, so a reply cannot render as something other than what
  it says;
- the engine runs as a child process bound to `127.0.0.1`, spawned with an
  argument vector and never through a shell.

The system prompt forbids inventing numbers and states plainly that the model
cannot act, but that is a quality measure, not the security control. The
security control is that there is no code path from model output to a
filesystem operation.

---

## 10. Other hardening

**Path traversal.** Normalised lexically before any comparison; `..` above the
root is dropped rather than allowed to escape.

**Long paths.** Operations are issued through a `\\?\` prefix so `MAX_PATH`
cannot truncate a path into a different one.

**Hostile filenames.** Control characters and invisible Unicode reordering
marks (U+202E and friends) are refused outright. `invoice<U+202E>cod.exe`
renders as `invoiceexe.doc`, and a storage utility must not present that
without comment.

**Shell invocation.** There is none. Uninstall commands come from the registry
as a full command line and are split into a program and an argument string,
then passed to `ShellExecuteW`. A registry value containing `& calc.exe` ends
up as an argument, not a second command; there is a test for exactly that.
`explorer.exe` is likewise started with its own argument vector.

**Privilege.** AllInsight runs unelevated and never requests elevation at launch.
Elevation is offered only where it unlocks something specific, always with a
reason, and always at the user's initiative.

**Symlink and junction attacks.** Refused at three levels: the scanner does not
follow them (so they are counted once, where the data really lives), the guard
refuses the entry itself, and the guard refuses any path reached through one.

**Race conditions.** Every check is re-run at the moment of deletion. The
window between validation and `remove_file` cannot be closed entirely on
Windows without opening a handle with delete-on-close semantics, but the
combination of canonicalisation, the allow-list, and refusing reparse points
means a winning race still lands inside a directory AllInsight was already
permitted to clean.

**Local surface.** No IPC endpoint is exposed outside the process. The only
socket in the application is an outbound connection to a loopback port owned by
the model process AllInsight itself spawned.

**Dependencies.** SQLite is compiled in rather than loaded as a DLL, which
removes a DLL-planting vector and makes the installer self-contained. The HTTP
client is built without TLS, so it cannot reach a remote host even by mistake.

---

## 10a. Linux

The same guard, tokens and category enum run on Linux. What differs is what
they are given:

- **Protected roots** come from a per-platform list: every top-level system
  directory (`/usr`, `/etc`, `/var`, `/boot`, `/opt`, `/root`, `/proc`, `/sys`,
  `/dev`, `/run`, `/snap`, `/nix`, ...) is refused with `SystemDirectory`, plus
  the Linux homes of secrets and profiles (`~/.mozilla`, `~/.thunderbird`,
  `~/snap`, `~/.var/app`, `~/.local/share/keyrings`, `~/.password-store`,
  `~/.pki`, `~/.kube`, `~/.docker`, `~/.local/share/Trash`). The first Linux
  build had none of this: every Windows root is a `%VAR%` template that expands
  to nothing on Linux, which is why the list is now split per platform rather
  than shared.
- **Comparison is case-sensitive** on Linux (`paths::CASE_INSENSITIVE`), so a
  frontend cannot name `Film.mkv` to reach a scanned `film.mkv`. Deny rules on
  names and extensions stay case-insensitive, which only ever over-protects.
- **`/tmp` is shared.** Cleanup only considers regular files and folders owned
  by the current user; sockets, pipes, other users' files and the system's
  private directories are not candidates at all.
- **The one carve-out** is Snap Firefox's cache, the only disposable folder
  inside the otherwise protected `~/snap`.
- **Walkers do not cross into** kernel, memory, image, container or network
  filesystems (`services::storage::fence`), and skip every non-regular file,
  so a scan never opens a FIFO or reports `/proc/kcore` as a large file.
- **AllInsight never runs as root** and never starts a package manager with
  privilege. Flatpak and Snap removals go through those tools as the user;
  apt, dnf, zypper and pacman removals are shown as a command to run.

## 10b. Updates

The updater is the only code that reaches the internet, and the only code
that can cause a new program to run. It is built so that compromising the
download server is not enough to compromise users. Full design:
[UPDATE_SYSTEM.md](UPDATE_SYSTEM.md).

- **HTTPS, to trusted hosts only.** Every URL goes through
  `update::config::check_url`: the metadata address, every redirect hop
  (redirects are followed by hand, never by the library), and the package
  address. It refuses `http://`, other schemes, hosts outside the compiled
  allowlist, look-alike hosts, and URLs carrying credentials. The HTTP agent
  is also built with `https_only(true)`.
- **Trusted endpoint, fixed at build time.** The update address and the
  trusted key are compile-time constants. No setting, settings import or
  environment variable at run time can change them.
- **Signed metadata.** `latest.json` must verify against the minisign / `tauri
  signer` public key compiled into the binary *before it is parsed*. If this
  build has no key, the updater makes no request at all. Signed metadata is
  then checked again: right product, right channel, a supported schema,
  valid versions, and never a downgrade.
- **SHA-256.** The package must hash to the value in the signed metadata. A
  mismatch deletes the file and reports *Update verification failed. For your
  security, the update was not installed.* The hash is checked again
  immediately before the installer runs, because the file sat on disk in the
  meantime.
- **Digital signatures (Windows).** `WinVerifyTrust` checks the installer's
  Authenticode signature, including the revocation of the whole chain. A
  broken or untrusted signature is always refused. When the build sets
  `ALLINSIGHT_UPDATE_PUBLISHER`, the installer must be validly signed by
  exactly that publisher, or it is refused.
- **No silent install.** Nothing downloads without *Update Now*, and nothing
  installs or restarts without *Restart Now*.
- **Rollback and safe installation.**
  - The database is backed up before every install, and the install doesn't
    start if the backup fails.
  - Schema migrations run in one transaction after a full backup.
  - The NSIS installer runs in update mode, where the uninstall hook keeps
    user data.
  - An AppImage swap restores the previous file if it fails.
  - A Windows install that is interrupted is repaired by re-running the
    verified installer, which is kept until the new version starts.

## 11. What is not claimed

- Until a code-signing certificate is in place, the installer is unsigned and
  Windows will warn on first run. Updates are still verified by the signed
  metadata and its SHA-256.
- An NSIS installer killed halfway is not transactional. AllInsight detects it
  on the next launch and keeps the verified installer to run again, but it
  does not undo partial file copies itself.
- AllInsight is not anti-malware and makes no claim about the trustworthiness of
  the files it lists.
- Drive health is only as good as what the drive reports. Where the counters
  are unreadable, AllInsight reports `Unknown` rather than guessing, which is a
  deliberate refusal to be helpfully wrong.
- Startup impact is estimated from the executable's size, because Windows does
  not publish Task Manager's measured value. The interface says so on every
  row.
