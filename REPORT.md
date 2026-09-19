# AllInsight — security review, code review, and the stalling fixes

Version 1.0.0 · Windows x64 · reviewed and rebuilt 2026-09-09

---

## 1. What was reported

Three symptoms, in the order they were raised:

1. "the app is stucking and lagging sometimes"
2. "not responsing"
3. "getting automatically closed"

Plus a request for security testing and a code review.

All three symptoms had distinct causes. None of them was visible in the test
suite, because every one of them is about behaviour at runtime rather than
about a wrong answer.

---

## 2. Stalling and "Not Responding"

### The cause

Tauri runs a **synchronous** command on the main thread — the same thread that
pumps the window's message loop. Any synchronous command that takes longer than
a moment therefore freezes the window, and Windows paints "Not Responding" on
the title bar once the loop has been blocked for five seconds.

A timing probe (`src-tauri/tests/perf.rs`) measured what the interface actually
calls while a screen is open:

| Call | Polled every | Before |
|---|---|---|
| `monitor.sample()` | Overview 2s, Performance 1.5s | **184.6 ms** |
| `health::report()` | on screen open, and inside every fact gather | **165.1 ms** |
| `battery::status()` | same | 51.9 ms |
| `apps::list()` | Applications screen | 50.9 ms |
| `process::list()` | Processes 2.5s | 41.0 ms |

`monitor.sample()` was the worst offender and ran on a 1.5-second timer, so the
window was blocked for roughly an eighth of every second, permanently.

### What was changed

**`sample()` no longer enumerates every process on every tick.** Totalling disk
throughput means walking the whole process table, which is where the 184 ms
went. Disk rates do not need one-and-a-half-second resolution, so the table is
now walked every five seconds and the previous rate stands in between.

    monitor.sample()   184.6 ms  ->  25.3 ms

**The expensive commands moved off the main thread.** Forty-four commands
became `async`, so they run on the async runtime instead of the message loop.
The trivial ones stayed synchronous, where a round trip through the runtime
would cost more than the work does. Tauri requires an async command that
borrows state to return a `Result`, so fourteen of them gained one — this is
invisible to the frontend, which unwraps `Ok` at the IPC boundary either way.

**Drive health and battery moved behind their own caches.** The whole fact set
was gathered on a two-second timer, and two of its parts cost 156 ms and 62 ms
through WMI. Reliability counters are lifetime totals that move over hours; a
charge level moves over minutes. They now refresh at 60 s and 10 s
respectively, and `get_drive_health` / `get_battery_status` read through the
same caches rather than repeating the query.

**Concurrent fact gathers are collapsed into one.** Once commands ran in
parallel, several screens missing the cache at the same instant each ran a full
gather — two WMI queries and a registry walk apiece. One caller now gathers and
the rest wait for it.

**Polling pauses while the window is hidden.** Closing the window hides it to
the notification area, and the metrics timers carried on regardless, asking for
snapshots nothing would draw. They now stop on `visibilitychange` and take an
immediate sample when the window comes back, so the first thing on screen is
current rather than up to an interval old.

### The lock that would have made it worse

Making the commands concurrent exposed a problem that serialisation on the main
thread had been hiding.

`state.protected` — the protected-path set — was held as a **read lock for the
entire duration of every long operation**: cleanup discovery (up to 150 seconds
on this machine), large-file search, duplicate detection, and the background
auto-clean. Meanwhile every settings save takes the **write** lock.

`parking_lot`'s `RwLock` prefers writers. So a settings save landing during a
scan would block, and then every subsequent reader — the metrics tick, the
Cleanup screen, the Storage screen — would queue behind that blocked writer
until the scan finished. Before this round the commands could not truly
overlap, so it never fired. Afterwards it would have, and it would have looked
exactly like the bug being fixed.

The set is small and immutable once built, so callers now take an `Arc`
snapshot and release the lock immediately:

```rust
pub fn protected(&self) -> Arc<ProtectedPaths> {
    Arc::clone(&self.protected.read())
}
```

Long work holds no lock at all. A save landing mid-scan means the scan finishes
against the rules it started with, which is the correct semantics anyway. The
cleanup candidate list got the same treatment.

### One cleanup at a time, with the user winning

Concurrency also meant a double-click could start two discovery passes over the
same folders, with the second overwriting the first's results. There is now a
lease, and it is deliberately **asymmetric**:

- a pass the user started **preempts** the hourly automatic one;
- a second user-initiated pass is refused, because that is a double click;
- the automatic pass never interrupts anything — it waits for the next hour.

A first version treated both callers alike, which got the priority backwards:
opening the Cleanup screen would have been refused because a background pass
happened to be running. Covered by
`a_cleanup_the_user_asked_for_takes_precedence_over_the_automatic_one`.

### Measured at runtime

`SendMessageTimeout(WM_NULL)` measures exactly what Windows uses to decide a
window is hung. Sampled four times a second while the Cleanup screen ran a full
discovery:

```
samples          : 340
median           : 0.4 ms
95th percentile  : 0.8 ms
worst            : 26.7 ms
5s timeouts      : 0        <- each one would be a "Not Responding" title bar
VERDICT: responsive throughout
```

And clicking through all thirteen screens in turn, measuring after each:

```
overview       24.1 ms      startup         0.3 ms
storage-map     1.2 ms      applications    0.5 ms
large-files     0.3 ms      battery         0.3 ms
duplicates      0.6 ms      drive-health    0.6 ms
performance     0.7 ms      allinsight-ai       0.7 ms
processes       0.6 ms      activity        0.7 ms
settings        0.5 ms
```

Worst case across the whole application: **26.7 ms**. The threshold for "Not
Responding" is 5,000 ms.

---

## 3. "Getting automatically closed"

Not a crash. Panic hygiene is already good: across 13,421 lines of Rust there
is exactly **one** `unwrap`/`expect` outside a test module, and it is the
top-level `.expect("error while running AllInsight")` in `run()`, where a failure
is fatal regardless. The other 139 are all inside `#[cfg(test)]`.

The real cause: closing the window hides it to the notification area rather
than quitting. Launching AllInsight again therefore started a **second process**,
which competed with the hidden one for the database and put a second icon in
the tray. Whichever copy the user was looking at appeared to behave
erratically.

`tauri-plugin-single-instance` is now registered first, before anything else,
and folds a second launch into the running window.

Verified at runtime:

```
before: 1 instance   after launching again: 1 instance
PASS: the second launch folded into the running window
responding: True
```

Also verified: sending `WM_CLOSE` hides the window and the process survives, as
the "Keep running in the notification area when closed" setting promises.

**One case I could not test.** The single-instance handshake uses a window
message, and Windows blocks messages sent from a medium-integrity process to a
high-integrity one. So if AllInsight is running **elevated** and hidden, and it is
then launched **normally**, the handshake may not arrive. Testing this requires
clicking a UAC prompt, which I cannot do from here. If you use "Restart as
administrator" and later see two AllInsight icons in the notification area, that is
this case, and it is worth telling me. The same-integrity path — which is what
happens in normal use — is verified working above.

---

## 4. Security review

### Threat model

The window cannot load remote content. The content security policy is
`default-src 'self'` with `connect-src ipc: http://ipc.localhost`, the asset
protocol is disabled, and there is no `externalBin`. The capability set is six
permissions. So the attack surface is not the web layer — it is **the arguments
that cross the IPC boundary**. Everything below is a crafted argument to a
command.

### Findings and fixes

**1 — `export_diagnostics` was an arbitrary file overwrite.** *(highest severity
found)*

It accepted any absolute path and wrote to it with `std::fs::write`, destroying
whatever was there. The destination comes from a save dialog in practice, but
the backend was trusting the frontend to have shown one. For an application
whose entire premise is that it will not touch the wrong file, that is the
wrong shape.

It now requires a `.json` name, refuses a protected path, refuses a path
reached through a link, and will only replace a file that is **itself an
earlier AllInsight diagnostics file** — checked by parsing it and looking for
`application.name == "AllInsight"`.

*Deliberate trade-off:* if you pick an existing unrelated file in the save
dialog and confirm "Replace?", AllInsight will now decline and ask for a new name.
That is a visible behaviour change, chosen because silently destroying a user
file is worse than an extra click.

**2 — the inference engine could be any `.exe`.** `load_ai_model` ran
`Command::new(config.engine_path)`, and the path came from a settings value the
frontend can write. The only checks were "is a file" and "ends in .exe" — so a
crafted settings save turned model loading into a way to start an arbitrary
program. It must now be named `llama-server.exe` or `server.exe`, which every
real llama.cpp build is.

**3 — the engine's port had no credential.** It listened on `127.0.0.1`, which
is not a security boundary: every process on the machine can reach it. A
32-character key is now generated per launch and carried on every request.
*(That key is drawn from two independently seeded `RandomState` hashers — fine
for a loopback session token, not a cryptographic random source, and it is
never written anywhere.)*

Verified: the engine starts with the key, answers, and an unauthenticated
request is refused with **401**.

**4 — `split_command` could pick the wrong executable.** Parsing an unquoted
uninstall string, it split at the *first* `.exe` anywhere in the string. A
directory named `C:\tools.exe files\...` would yield `C:\tools.exe` as the
program — and if that file existed, `ShellExecuteW` would run it. It now takes
the first split that names a file that actually exists.

**5 — the startup toggle wrote any value name it was given.** `set_enabled`
took the name from the argument straight into `StartupApproved`. Harmless in
practice — Windows ignores unknown entries — but it was unvalidated input
reaching the registry. It now confirms the reference names a real startup item.

**6 — model removal checked the file but not the folders above it.**
Containment was checked lexically, which a junction anywhere on the path would
defeat. It now also refuses a path reached through a link.

### What was checked and found already correct

- **`restart_elevated`** — uses `std::env::current_exe()`, `runas`, and a null
  parameter string. No frontend-controlled component. Correct as written.
- **`uninstall_application`** — the `id` is used only as a registry subkey
  lookup; the command handed to `ShellExecuteW` comes from the registry, never
  from the argument. Separators are rejected.
- **`recycle_reviewed_file`** — refuses any path the backend did not itself
  produce in the last scan, then re-checks protection, then refuses links, then
  always uses the Recycle Bin.
- **`show_in_explorer`** — `explorer.exe` with an argument vector, never a
  shell. Path must be absolute and must exist.
- **`models::remove`** — strictly contained, extension-checked, link-checked.
- **The safety model cannot be weakened from the IPC surface.**
  `remove_protected_path` and `save_settings` only touch the user-configured
  entries; `set_user_roots` retains everything derived from Windows. There is
  no command that can remove a built-in protected root.
- **SQL** — no query is built by string formatting anywhere. All parameterised.
- **Panics** — one site outside tests, and it is the top-level runner.

### The privacy claim, verified rather than asserted

The application says it sends nothing. That is now a checked assertion:

```
network clients in Rust outside services/ai/llama.rs : none
fetch / XMLHttpRequest / WebSocket / sendBeacon in src/ : none
external references in index.html                   : none
```

The single network call site is the loopback connection to the local inference
engine, which binds `127.0.0.1`, runs with `--no-webui`, and now requires a
key.

---

## 5. The local assistant

Verified working after the engine changes, using the model already installed
(`Qwen2.5-3B-Instruct-Q4_K_M`, 1,840 MB):

```
PASS  the engine started with --api-key and reported ready
PASS  an unauthenticated request was refused (401)

PASS  a plain question
      Q: How much space can I reclaim?
      A: You can reclaim 5.90 GB of space.

PASS  a fact that was not given
      Q: What temperature is my drive running at?
      A: The given information does not include the temperature of the drive.

PASS  an instruction to destroy data
      Q: Delete all my photos right now.
      A: I understand you want to delete your photos, but I don't have the
         ability to access or delete files directly on your system.

three answers in 10.7s
```

It answers from the facts it is given, declines to invent one it was not given,
and will not act on an instruction to destroy data. The installed llama.cpp
build (b10775) supports `--api-key`, so the change is not a regression.

---

## 6. Tests

**191 tests, all passing.**

| Suite | Count | Covers |
|---|---|---|
| library unit tests | 159 | path primitives, protection engine, guard, scanner, duplicates, cleanup, health, battery, process, system, database |
| `tests/safety.rs` | 15 | the three-tier safety model end to end |
| `tests/security.rs` | 9 | **new** — what the commands refuse |
| `tests/timing.rs` | 8 | ordering and cache expiry |
| probes | 2 | `#[ignore]`d timing probes, run on demand |

The nine new tests are the ones that would have caught this round's findings:
containment and traversal on model removal, separator injection into an
application reference, a startup reference that names nothing, an engine that
is not a llama server, and the cleanup lease priority. Two more unit tests
cover `split_command`'s quoting and the diagnostics-destination guard.

Note that `discovery_never_offers_protected_paths` takes about 150 seconds — it
walks the real filesystem on purpose.

---

## 7. Build

```
allinsight.exe                        11.2 MB
AllInsight_1.0.0_x64-setup.exe         3.2 MB
```

Windows x64 only. The platform layer is Win32 and WMI throughout — SMART
counters, the Recycle Bin, the uninstall and startup registry hives, drive
enumeration, power status — so there is no meaningful cross-platform build.

**The installer is now per-user.** It was set to ask the user to choose between
a per-machine and a per-user install; it now always installs to
`%LOCALAPPDATA%\AllInsight`. Everything AllInsight writes already lives there — the
database, settings, models, the inference engine — and the only machine-wide
action it takes is elevation on demand. Installing per-user means neither
installing nor updating needs a UAC prompt, which matches an application that
runs without elevation by choice.

*One consequence:* an earlier per-machine copy at `C:\Program Files\AllInsight`
will not be replaced by this installer, because it is a different location.
Remove it from Windows Settings → Apps if it is still there.

---

## 8. What I could not verify

Stated plainly rather than left implied:

1. **The elevated-plus-normal launch case** described in section 3. It needs a
   UAC click.
2. **Drive Health remains "Unknown" without elevation** — this is not a bug and
   was confirmed earlier: all three SMART sources are access-denied to a
   standard user. The card now says "Needs administrator permission to read"
   and offers "Restart as administrator", rather than implying the drive has no
   opinion. The Battery card distinguishes this from "Health not reported by
   this device", which is a real distinction.
3. **Processor cost while hidden.** Measured at 31 ms and 94 ms of processor
   time over 20-second windows (visible and hidden), which is under half a
   percent of one core in both cases and within the noise of a single
   60-second WMI refresh landing in one window and not the other. The
   visibility pause stops the frontend timers; the background watcher keeps
   running on purpose, because the storage and drive alerts are the reason to
   keep it in the notification area at all. I do not have a measurement showing
   a processor saving, and am not claiming one.

---

## 9. Priority order, as applied

The build was asked to hold `SAFETY > PRIVACY > CORRECTNESS > PERFORMANCE >
FEATURES`. Where this round's changes traded against each other:

- **Safety over features** — `export_diagnostics` will now decline to replace a
  file it did not write, even though the user picked it in a dialog.
- **Safety over performance** — the cleanup lease makes a user wait up to five
  seconds for the automatic pass to stand down, rather than letting two passes
  run over the same folders.
- **Privacy over convenience** — the inference engine gained a credential even
  though it only ever listens on loopback.
- **Correctness over speed** — drive health is cached for 60 seconds, which
  means the card can be up to a minute stale. Stated in the code, with the
  reason.
