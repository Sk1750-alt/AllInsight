# AllInsight architecture

## Shape

```
┌─────────────────────────────────────────────────────────────┐
│  WebView2 window                                            │
│  React + TypeScript. Renders state and collects intent.     │
│  Holds no privilege and makes no decision that matters.     │
└───────────────────────────┬─────────────────────────────────┘
                            │  Tauri IPC, typed both sides
┌───────────────────────────▼─────────────────────────────────┐
│  commands/                                                  │
│  Validates arguments, calls one service, returns data.      │
│  Deliberately thin: no logic lives here.                    │
└───────────────────────────┬─────────────────────────────────┘
┌───────────────────────────▼─────────────────────────────────┐
│  services/                                                  │
│  security  storage  cleanup  system  process                │
│  health    battery  apps     startup  ai       db           │
└───────────────────────────┬─────────────────────────────────┘
                            │  Win32, WMI, registry, filesystem
┌───────────────────────────▼─────────────────────────────────┐
│  Windows                                                    │
└─────────────────────────────────────────────────────────────┘

           ┌──────────────────────────────────┐
           │  llama-server.exe (child process)│
           │  127.0.0.1 only. Optional.       │
           └──────────────────────────────────┘
```

Three properties follow from this shape, and each is worth stating:

1. **The interface holds no privilege.** Every destructive decision is made in
   Rust, from backend-held state. A compromised or simply buggy frontend
   cannot delete anything.
2. **Inference is out of process.** A model that hangs, thrashes or runs out
   of memory cannot take the window with it, and AllInsight's own binary contains
   no inference code and no weights.
3. **Nothing is on the network.** There is no HTTP client in the binary
   capable of reaching a remote host: the one that exists is compiled without
   TLS and is only ever pointed at a loopback port.

---

## Startup sequence

Ordered so the window is useful immediately.

1. Initialise logging into `%LOCALAPPDATA%\AllInsight\logs`.
2. Open SQLite, run migrations, load settings.
3. Build the protected-path engine from Windows known folders plus the user's
   additions.
4. Create the metrics sampler and the (idle) model engine.
5. **Render.** The shell and the current screen draw from what is already
   known.
6. Start the background monitor after a five second delay, so its first tick
   does not compete with the window appearing.
7. Screens fill themselves in. Nothing waits on a filesystem scan.

A model set to load automatically starts on its own thread, so a five gigabyte
GGUF never delays the first frame.

---

## The scanner

One walk produces everything the storage screens need: the directory tree with
rolled-up sizes, per-category totals, and the largest files.

- **Parallel** through `rayon`, recursing into sibling directories
  concurrently. Aggregation happens on the way back up, so totals come out of
  the recursion rather than needing a second pass.
- **Bounded memory.** Node detail stops at `max_node_depth` (8 by default);
  deeper bytes still roll up into their ancestors, so totals stay exact while
  the node count stays proportional to the shallow part of the tree.
- **Cancellable.** An `AtomicBool` checked per directory and per entry.
  Cancelling returns a partial but internally consistent result, not an error.
- **Tolerant.** A directory that cannot be read increments a counter and the
  walk continues. Permission denied is the *normal* case when scanning `C:\`
  unelevated, not an exceptional one, and the count is surfaced so the user
  knows the total excludes something.
- **Link-aware.** Reparse points are recorded and not followed, so a junction
  does not cause the same bytes to be counted twice.

The treemap never ships the whole tree to the frontend. Drilling down asks for
exactly one level, which is what keeps the screen usable on a volume with
millions of files.

---

## Duplicate detection

Three passes, each cheaper than the next is expensive:

1. **Group by exact size.** One `read_dir` pass. No file is opened. A size
   seen once cannot be a duplicate.
2. **Partial hash.** BLAKE3 over the first and last 64 KB plus the length.
   Two small reads eliminate almost every remaining candidate.
3. **Full hash.** Streamed in 1 MB chunks, only for files that survived pass
   two.

Names are never evidence. Two files match only when their contents hash
identically, which is why the test suite includes a pair that differs solely in
the middle of the file: partial hashing alone would call them duplicates, and
does not.

---

## Metrics

One `sysinfo::System` lives for the process lifetime and is refreshed on a
timer. Rebuilding it per request would be slower *and* wrong, because CPU
percentages are computed from the delta between two refreshes.

Rates come from cumulative counters. A counter that goes backwards (an
interface disappeared, a process exited) yields zero rather than a nonsense
spike. Ten minutes of samples are kept in a ring buffer, in memory only.

GPU utilisation comes from WMI performance counters, polled every fifth tick
because it is far more expensive than the rest. When the counters are absent -
and they are, on plenty of machines - the card reports `null` and the interface
says "not available" rather than drawing a flat line at zero.

---

## Drive health

Two layers, and the gap between them is the interesting part:

| Layer | Source | Elevation |
| --- | --- | --- |
| Model, bus, media, capacity, Windows' own verdict | `MSFT_PhysicalDisk` | No |
| Wear, temperature, power-on hours, error counts | `MSFT_StorageReliabilityCounter` | Usually yes |

When the second layer is unreadable, the state is `Unknown`, with an
explanation and an offer to elevate. It is specifically **not** `Healthy`:
Windows having no complaint is not the same as the drive reporting it is well,
and presenting one as the other is the failure mode this whole screen exists to
avoid.

An uncorrected read or write error, a predictive failure, or heavy wear always
downgrades the verdict, whatever else is true.

---

## The AI layer

Two layers, in this order:

**Deterministic insights.** Template-filled from measurements, by rules that
can be read and tested. Every sentence on the Overview - the score, its
reasons, the recommendations, the summary - comes from here. This is what makes
"works fully without a model" true rather than aspirational.

**The local model.** Optional, conversational, and given a text briefing built
from the same facts. It explains; it does not decide. The suggested actions
under an answer are chosen by `actions_for(facts)` from measurements, never
parsed out of what the model wrote.

The engine is a llama.cpp `llama-server` child process spoken to over
loopback HTTP. That choice makes the engine swappable, keeps inference out of
the UI process, and keeps model weights out of AllInsight's binary.

---

## Persistence

One SQLite file, compiled into the binary so the installer ships no loose DLL.

| Stored | Not stored |
| --- | --- |
| Settings | File names |
| Cleanup history as totals and category names | File contents |
| Scan history: root, bytes, files, duration | File hashes |
| Volume capacity over time, ~400 samples per volume | Assistant questions or answers |
| An event log of what AllInsight did | Anything from another machine |

WAL mode, so the background monitor's writes never block the interface's
reads. Timestamp ordering is tie-broken by row id, because two rows written in
the same second are otherwise ordered arbitrarily.

---

## Background monitor

One thread, waking on a user-controlled interval, doing the least work that
keeps the alerts honest: read volume capacity, record a sample, check
thresholds, check drive health. It never walks the filesystem, so its cost is
flat regardless of how many files exist.

Only the highest crossed threshold fires, so passing 80 and 90 in one step
produces one message rather than two, and a quiet period stops AllInsight repeating
itself.

Auto-Clean runs from here, inside the boundaries described in
[SECURITY.md](SECURITY.md): the compiled-in eligible set, intersected with what
the user opted into, only below their free-space threshold, only when no
user-initiated scan is running, and only when there is at least 256 MB to
reclaim - because interrupting someone for less than that is not worth it.

---

## Error handling

`AllInsightError` distinguishes failure classes so callers can branch on them,
and in particular so `AccessDenied` is never treated as fatal. Errors serialise
across IPC as the message string, and those messages are written for people:
the interface shows them directly rather than substituting its own wording.

Screens are wrapped in an error boundary. A screen that throws does not take
the window with it; the shell stays alive and the user can navigate elsewhere.

---

## Frontend

- **State.** One small context: settings, current screen, scan progress,
  toasts. Screen data is fetched by the screen that needs it, so opening one
  view never pays for the others.
- **Fetching.** `useAsync` for one-shot loads, `usePolled` for live metrics.
  Both handle cancellation on unmount; a single failed poll is ignored rather
  than interrupting the user.
- **Theming.** CSS custom properties on `:root`. Light and dark define the same
  token names, so no component knows which is active.
- **Charts.** Hand-drawn SVG. They are simple shapes that must match the
  palette exactly, and a charting library would ship its own theme to fight
  with on every screen.
- **Treemap.** Squarified layout (Bruls, Huizing, van Wijk), because squares
  are far easier to compare by eye than the slivers a slice-and-dice layout
  produces. Tiles below 0.4% of the parent are folded into one "smaller items"
  tile rather than drawn as invisible slivers.

---

## Extending it

The module boundaries are the extension points. A new capability means a new
`services/` module, a thin `commands/` wrapper, and a screen. Specifically:

- **A new cleanup category** is a new enum variant plus one `CategoryDefinition`
  in `cleanup/categories.rs`. If its root is inside a protected location it
  also needs a carve-out entry, and the test in that module will fail until it
  has one. That failure is the design working.
- **A different AI engine** is a new implementation behind the same
  start/complete/stop shape in `ai/`. Nothing outside that module knows what
  llama.cpp is.
- **A new metric** is a field on `SystemSnapshot` and a card on Performance.

The one rule: anything that removes data goes through
`security::guard::DeletionGuard`, and takes a `ValidatedPath`. There is no
second way in, and adding one would be the mistake.
