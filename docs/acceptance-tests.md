# imgcull — Manual Acceptance Tests

This document defines the manual acceptance test suite for imgcull. Run these tests before any release or after significant changes to the pipeline, XMP handling, or CLI.

Each test is self-contained. Tests that write files use a temporary working directory (`~/tmp/imgcull-test/`) — create and clean it up before and after the session.

---

## Setup

```bash
# Build the release binary
cargo build --release
alias imgcull="$(pwd)/target/release/imgcull"

# Create a clean test workspace
mkdir -p ~/tmp/imgcull-test
cd ~/tmp/imgcull-test
```

Prepare two test images in the workspace:

- `small.jpg` — any JPEG under 2 MP (should be passed through without resize)
- `large.jpg` — any JPEG over 2048px on its longest edge (should be resized before sending)
- `raw.CR2` or `raw.NEF` — a RAW file (any camera)  *(optional — skip RAW tests if unavailable)*

Ensure at least one provider API key is set, e.g.:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

---

## AT-01 · First-time init

**Goal:** Verify `imgcull init` creates all expected config files.

**Steps:**

1. Remove any existing imgcull config: `rm -rf ~/Library/"Application Support"/imgcull`
2. Run `imgcull init`

**Expected result:**

- `~/Library/Application Support/imgcull/config.toml` exists and is valid TOML
- `~/Library/Application Support/imgcull/prompts.toml` exists and is valid TOML
- `~/Library/Application Support/imgcull/.env.example` exists and contains placeholder key names (e.g. `ANTHROPIC_API_KEY=`)
- No error messages in output

**Pass criteria:** All three files present; `imgcull init` exits 0.

---

## AT-02 · Dry-run produces no side effects

**Goal:** Verify `--dry-run` makes no LLM calls, writes no files, and requires no API keys.

**Steps:**

1. Unset all provider API keys: `unset ANTHROPIC_API_KEY OPENAI_API_KEY`
2. Run:
   ```bash
   imgcull score --dry-run ~/tmp/imgcull-test/small.jpg
   ```
3. Check for sidecar: `ls ~/tmp/imgcull-test/small.xmp`

**Expected result:**

- Command exits 0 (or with a clear "dry run" message)
- No `small.xmp` file is created
- No error about missing API key

**Pass criteria:** Exit 0, no `.xmp` written.

---

## AT-03 · Score writes correct XMP fields

**Goal:** Verify that scoring a JPEG creates an `.xmp` sidecar with all expected imgcull fields.

**Steps:**

1. Run:
   ```bash
   imgcull score ~/tmp/imgcull-test/small.jpg
   ```
2. Inspect the output sidecar:
   ```bash
   cat ~/tmp/imgcull-test/small.xmp
   ```

**Expected result:**

- `small.xmp` is created alongside `small.jpg`
- Contains `<dc:description>` with a non-empty text value
- Contains `xmp:Rating="N"` where N is 1–5
- Contains `<imgcull:score>` with a value between `0.00` and `1.00`
- Contains at least one `<imgcull:sharpness>` (or other dimension) element
- Contains `<imgcull:scored_at>` (ISO-8601 timestamp)
- Contains `<imgcull:scored_by>` (provider/model string, e.g. `claude/claude-opus-4-5`)
- Contains `<imgcull:original_filename>` with the source image filename (e.g. `small.jpg`)
- Contains `<imgcull:dimensions>` listing the scored dimensions (e.g. `sharpness,exposure,composition,subject_clarity,aesthetics`)
- Contains `<imgcull:scoring_response>` with a narrative critique from the LLM
- Terminal output shows the filename with a star display (e.g. `★★★☆☆`)

**Pass criteria:** All fields present and within valid ranges.

---

## AT-04 · Describe-only writes description but no score fields

**Goal:** Verify `imgcull describe` skips scoring and leaves no score fields in the sidecar.

**Steps:**

1. Remove any existing sidecar: `rm -f ~/tmp/imgcull-test/small.xmp`
2. Run:
   ```bash
   imgcull describe ~/tmp/imgcull-test/small.jpg
   ```
3. Inspect `small.xmp`.

**Expected result:**

- `<dc:description>` is present and non-empty
- No `xmp:Rating` attribute
- No `<imgcull:score>` element
- No `<imgcull:scored_at>` element

**Pass criteria:** Description present; all score fields absent.

---

## AT-05 · `--no-description` skips description, writes scores

**Goal:** Verify the `--no-description` flag suppresses description generation.

**Steps:**

1. Remove existing sidecar.
2. Run:
   ```bash
   imgcull score --no-description ~/tmp/imgcull-test/small.jpg
   ```
3. Inspect `small.xmp`.

**Expected result:**

- No `<dc:description>` element
- `xmp:Rating` is present
- `<imgcull:score>` is present

**Pass criteria:** Description absent; score fields present.

---

## AT-06 · `--no-rating` skips star rating

**Goal:** Verify `--no-rating` suppresses writing `xmp:Rating`.

**Steps:**

1. Remove existing sidecar.
2. Run:
   ```bash
   imgcull score --no-rating ~/tmp/imgcull-test/small.jpg
   ```
3. Inspect `small.xmp`.

**Expected result:**

- No `xmp:Rating` attribute anywhere in the file
- `<imgcull:score>` is still present

**Pass criteria:** `xmp:Rating` absent; `imgcull:score` present.

---

## AT-07 · Skip re-processing already-scored images

**Goal:** Verify that a second run without `--force` does not overwrite existing scores.

**Steps:**

1. Run `imgcull score ~/tmp/imgcull-test/small.jpg` (first run).
2. Note the `<imgcull:scored_at>` timestamp from `small.xmp`.
3. Run `imgcull score ~/tmp/imgcull-test/small.jpg` again (second run).
4. Check the `<imgcull:scored_at>` timestamp again.

**Expected result:**

- The timestamp is **unchanged** after the second run
- Terminal output for the second run indicates the image was skipped (no star display printed)

**Pass criteria:** Timestamp identical; no unnecessary LLM call made.

---

## AT-08 · `--force` re-processes already-scored images

**Goal:** Verify `--force` overwrites existing scores even when they are present.

**Steps:**

1. Ensure `small.xmp` exists with scores from a previous run.
2. Note the `<imgcull:scored_at>` timestamp.
3. Run:
   ```bash
   imgcull score --force ~/tmp/imgcull-test/small.jpg
   ```
4. Check the new `<imgcull:scored_at>` timestamp.

**Expected result:**

- The timestamp is **updated** to the current time
- A new star display is printed in the terminal

**Pass criteria:** Timestamp changed; scores refreshed.

---

## AT-09 · `--backup` creates `.xmp.bak` before modifying

**Goal:** Verify that `--backup` preserves the original sidecar.

**Steps:**

1. Ensure `small.xmp` exists with scores from a previous run.
2. Note the original content.
3. Run:
   ```bash
   imgcull score --backup --force ~/tmp/imgcull-test/small.jpg
   ```
4. Check for `small.xmp.bak`.

**Expected result:**

- `small.xmp.bak` exists
- Its content matches the **original** `small.xmp` (before the forced re-score)
- `small.xmp` contains the new scores

**Pass criteria:** `.bak` file present with pre-run content; `.xmp` updated.

---

## AT-10 · XMP merge preserves existing third-party metadata

**Goal:** Verify that a pre-existing Lightroom sidecar (with `lr:*` or `photoshop:*` fields) is not destroyed.

**Steps:**

1. Create a synthetic Lightroom sidecar:
   ```bash
   cat > ~/tmp/imgcull-test/small.xmp <<'EOF'
   <?xml version="1.0" encoding="UTF-8"?>
   <x:xmpmeta xmlns:x="adobe:ns:meta/">
     <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
       <rdf:Description
         xmlns:lr="http://ns.adobe.com/lightroom/1.0/"
         xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
         photoshop:ColorLabel="Red"
         lr:hierarchicalSubject="Travel|Paris">
       </rdf:Description>
     </rdf:RDF>
   </x:xmpmeta>
   EOF
   ```
2. Run `imgcull score ~/tmp/imgcull-test/small.jpg`.
3. Inspect `small.xmp`.

**Expected result:**

- `photoshop:ColorLabel="Red"` is still present
- `lr:hierarchicalSubject` is still present
- imgcull fields (`imgcull:score`, `xmp:Rating`, `dc:description`) are also present

**Pass criteria:** All pre-existing fields preserved; imgcull fields added.

---

## AT-11 · Directory recursion discovers all supported files

**Goal:** Verify that a directory path processes all supported formats and skips unsupported ones.

**Steps:**

1. Set up a test directory:
   ```bash
   mkdir -p ~/tmp/imgcull-test/batch
   cp ~/tmp/imgcull-test/small.jpg ~/tmp/imgcull-test/batch/a.jpg
   cp ~/tmp/imgcull-test/small.jpg ~/tmp/imgcull-test/batch/b.jpeg
   echo "not an image" > ~/tmp/imgcull-test/batch/readme.txt
   ```
2. Run (dry-run to avoid LLM cost):
   ```bash
   imgcull score --dry-run ~/tmp/imgcull-test/batch/
   ```

**Expected result:**

- `a.jpg` and `b.jpeg` are listed as candidates
- `readme.txt` is either silently ignored or produces a warning — it is **not** processed
- No error causes the command to exit non-zero

**Pass criteria:** 2 images found; `.txt` skipped gracefully.

---

## AT-12 · Provider override via `--provider`

**Goal:** Verify `--provider` routes both description and scoring to the named provider.

**Steps:**

1. Ensure a second provider is configured (e.g. `openai`) with a valid key.
2. Run:
   ```bash
   imgcull score --provider openai --verbose ~/tmp/imgcull-test/small.jpg
   ```
3. Inspect `<imgcull:scored_by>` in `small.xmp`.

**Expected result:**

- `<imgcull:scored_by>` contains `openai/` followed by the configured model name

**Pass criteria:** `scored_by` reflects the overridden provider.

---

## AT-13 · Missing API key produces a clear error

**Goal:** Verify that a missing API key produces an actionable error message, not a panic or obscure crash.

**Steps:**

1. Unset the active provider key: `unset ANTHROPIC_API_KEY`
2. Ensure `config.toml` uses `claude` as the default provider.
3. Run:
   ```bash
   imgcull score ~/tmp/imgcull-test/small.jpg
   ```

**Expected result:**

- Exit code is non-zero
- Stderr contains a message referencing the missing environment variable (e.g. `ANTHROPIC_API_KEY`)
- No panic or unwrap trace

**Pass criteria:** Clean error message; non-zero exit; no panic.

---

## AT-14 · Large image is resized before submission

**Goal:** Verify that an image wider or taller than 2048px is resized before base64 encoding.

**Steps:**

1. Prepare `large.jpg` (any JPEG with longest edge > 2048px).
2. Run with `--verbose`:
   ```bash
   imgcull score --verbose ~/tmp/imgcull-test/large.jpg
   ```

**Expected result:**

- Verbose output (or log) indicates the image was resized
- Command completes successfully; `large.xmp` is written with valid scores

**Pass criteria:** No error from oversized image; sidecar written.

---

## AT-15 · Ollama local provider (optional)

**Goal:** Verify that imgcull works end-to-end with a locally running Ollama instance.

*Skip if Ollama is not available.*

**Steps:**

1. Ensure Ollama is running: `ollama serve`
2. Pull a vision model: `ollama pull llava`
3. Set config to use ollama, or override on the command line:
   ```bash
   imgcull score --provider ollama ~/tmp/imgcull-test/small.jpg
   ```

**Expected result:**

- No API key errors
- `small.xmp` is written with scores
- `<imgcull:scored_by>` contains `ollama/llava`

**Pass criteria:** Full pipeline succeeds with no internet calls to external APIs.

---

## Teardown

```bash
rm -rf ~/tmp/imgcull-test
```

---

## Test Result Log

| Test | Date | Tester | Provider | Result | Notes |
|------|------|--------|----------|--------|-------|
| AT-01 | | | — | | |
| AT-02 | | | — | | |
| AT-03 | | | | | |
| AT-04 | | | | | |
| AT-05 | | | | | |
| AT-06 | | | | | |
| AT-07 | | | | | |
| AT-08 | | | | | |
| AT-09 | | | | | |
| AT-10 | | | — | | |
| AT-11 | | | — | | |
| AT-12 | | | | | |
| AT-13 | | | — | | |
| AT-14 | | | | | |
| AT-15 | | | ollama | | |
