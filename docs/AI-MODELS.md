# Local models in AllInsight

AllInsight ships no model and never downloads one. There is no download code in
the binary. Everything below is something you do deliberately, once.

**The application is fully functional without any of this.** The device score,
every insight, every recommendation and the Overview summary come from a
deterministic generator that reads measurements and fills templates. A model
adds conversation on top; it does not unlock features.

---

## 1. The engine

AllInsight speaks to llama.cpp's `llama-server` over loopback HTTP. Get a Windows
build from the llama.cpp releases page:

<https://github.com/ggml-org/llama.cpp/releases>

Pick a build matching your hardware:

| Build | For |
| --- | --- |
| `llama-*-bin-win-cpu-x64.zip` | Any x64 machine. Always works. |
| `llama-*-bin-win-cuda-*.zip` | NVIDIA cards, if you want GPU offload. |
| `llama-*-bin-win-vulkan-x64.zip` | AMD or Intel graphics. |

Extract it and put `llama-server.exe`, together with the DLLs beside it, into:

```
%LOCALAPPDATA%\AllInsight\engine\
```

AllInsight also looks in an `engine` folder next to `AllInsight.exe`, which is what a
portable copy should use, and Settings lets you point at an executable
anywhere.

---

## 2. The model

Any instruction-tuned GGUF works. **Size it to the disk as well as the memory** —
a machine that needs a storage manager is frequently a machine with no room for
a 5 GB file. AllInsight works this out for you and shows the answer on the AllInsight AI
screen; the ladder it picks from is:

| Parameters | Quantisation | On disk | Needs resident | Good for |
| --- | --- | --- | --- | --- |
| **0.5B** | Q4_K_M | ~400 MB | ~1 GB | The smallest useful option. Terse but coherent. |
| **1B** | Q4_K_M | ~810 MB | ~2 GB | A good balance when storage is tight. |
| **1.5B** | Q4_K_M | ~1.1 GB | ~3 GB | Noticeably better prose for a small extra cost. |
| **3B** | Q4_K_M | ~2 GB | ~4 GB | Comfortable when there is room to spare. |
| **7B–8B** | Q4_K_M | ~4.7 GB | ~6 GB | The best answers, if disk and memory allow. |

AllInsight takes the largest rung that fits **both** constraints: at most half of
installed memory, and free disk minus a 6 GB reserve. That reserve exists so
that installing a model can never be the thing that fills the drive. If nothing
fits, it says so and recommends nothing rather than suggesting a download that
would not help.

### Small models worth knowing about

These are the ones that actually fit on a constrained machine. All are
instruction-tuned and all answer perfectly well from a briefing of
measurements, which is the only job they have here:

| Model | Q4_K_M size | Notes |
| --- | --- | --- |
| **Qwen2.5-0.5B-Instruct** | ~400 MB | The best of the very small models. Follows instructions well for its size. |
| **SmolLM2-360M-Instruct** | ~270 MB | Smallest that stays coherent. Good on a nearly-full drive. |
| **Llama-3.2-1B-Instruct** | ~810 MB | Strong general quality per byte. |
| **Qwen2.5-1.5B-Instruct** | ~1.1 GB | Clearly better prose than 1B. |
| **Qwen2.5-3B-Instruct** | ~2.0 GB | The sweet spot when 2 GB is available. |
| **Phi-3.5-mini-instruct** (3.8B) | ~2.2 GB | Strong reasoning for the size. |

Going below Q4 is usually a false economy: `Q3_K_M` saves a few hundred
megabytes and costs noticeably more quality than moving down a parameter tier
does. Prefer a smaller model at Q4_K_M over a larger one at Q3.

### Reading the filename

`Meta-Llama-3.1-8B-Instruct.Q4_K_M.gguf`

- `8B` - parameter count, the main driver of both quality and memory.
- `Instruct` - tuned to follow instructions. Base models will not behave here.
- `Q4_K_M` - 4-bit quantisation, medium. The usual sweet spot: `Q5_K_M` is a
  little better and larger, `Q3` noticeably worse.

AllInsight parses these from the filename and shows them in Settings, along with
its own estimate of the memory needed to load the file.

---

## 3. Importing

**AllInsight AI → Import a GGUF model**, then pick the file.

AllInsight reads the first four bytes and refuses anything that is not really a
GGUF, however it is named. Valid files are copied into:

```
%LOCALAPPDATA%\AllInsight\models\
```

Dropping a `.gguf` into that folder by hand works too; it appears after a
refresh.

---

## 4. Loading

Press **Load**. The first load of a large model takes ten to sixty seconds
while it is read from disk.

Settings → AI:

| Setting | What it does |
| --- | --- |
| Context size | Tokens the model considers at once. 4096 is plenty for this task. |
| CPU threads | 0 lets AllInsight choose, leaving one core for the interface. |
| GPU layers | 0 keeps everything on the processor. Raise it only if your card has spare memory; guessing too high fails at load. |
| Load at startup | Off by default, so a large model never delays the first screen. |
| Keep loaded | Faster answers, at the cost of several gigabytes held permanently. |

**Unload and free memory** stops the child process and releases everything.

---

## 5. What the model is given

Exactly one block of text, built from measurements. **AllInsight AI → See what it
is given** shows it verbatim. It looks like this:

```
Drive C:\ (Windows): 476 GB total, 42.1 GB free, 91% used, holds Windows.
Videos: 96.4 GB.
Applications: 71.2 GB.
Downloads: 38.0 GB.
11.4 GB can be reclaimed from safe categories.
  Application temporary files: 4.20 GB.
  Crash dumps: 1.80 GB.
14 files are larger than 1 GB, totalling 38.2 GB.
Drive NVMe KINGSTON reports health Healthy, 92% of rated life remaining, 41 degrees Celsius.
Memory: 15.7 GB installed, 62% in use.
```

No file names. No file contents. No paths beyond the folders shown. Your
question follows this block, after an explicit `END OF MEASUREMENTS` marker so
a question cannot pass itself off as data.

---

## 6. What it cannot do

The model returns a string. That string is displayed. Nothing else happens to
it:

- no part of AllInsight parses model output for commands, paths or identifiers;
- the buttons under an answer are chosen from measurements, not from the text;
- output is stripped of control characters and Unicode bidirectional overrides
  and then capped, so a reply cannot render as something other than what it
  says;
- the engine process is spawned with an argument vector, never through a shell,
  and is bound to `127.0.0.1`.

The system prompt tells the model not to invent numbers and that it cannot act.
That is a quality measure. The security control is that there is no code path
from model output to a filesystem operation - see
[SECURITY.md](SECURITY.md) section 9.

---

## 7. When something goes wrong

**"The local inference engine was not found."** `llama-server.exe` is not in
`%LOCALAPPDATA%\AllInsight\engine\`. Check it is the executable itself and not a
folder containing it.

**"The local model did not finish loading within 120 seconds."** Usually a
model too large for available memory. Check the estimate shown next to it in
Settings; try a smaller quantisation.

**Answers are slow.** Expected on CPU: a 7B model at Q4 produces a few tokens
per second on a typical laptop. A smaller model is the fix, or GPU offload if
the card has the memory.

**It says something is not available.** That is correct behaviour. The model is
told not to invent numbers, and a figure AllInsight did not measure is not in the
briefing.

**Answers look generic.** Check the badge under the answer. "From
measurements" means the model was not used and you are seeing the deterministic
generator, which is the fallback whenever the engine is not loaded or fails.

---

## 8. Removing it

**Remove** next to a model deletes that file. The action is confined to the
managed models folder: it refuses any path outside it, anything that is not a
`.gguf`, and anything that is a link.

Turning off **Enable the local assistant** in Settings stops the engine and
releases its memory. The assistant screen keeps working, answering from
measurements.
