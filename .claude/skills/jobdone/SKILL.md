---
name: jobdone
description: >
  Wrap up a finished task: release build, commit and push to the current branch, put the
  debug marker next to the exe, then run the core image set plus the session's most-used
  original images through the exe into the PPRONI desktop folders. Manual only.
disable-model-invocation: true
argument-hint: "[commit message]"
allowed-tools: Bash(rtk git add *), Bash(rtk git commit *), Bash(rtk git push *)
---

# Job done

Run these four steps in order. If a step fails, stop and report it. Do not go on.

## 1. Build
`cargo build --release`. If the last step fails with "Access is denied" on
`target\release\pproni.exe`, the exe is open. Copy `target\release\deps\pproni.exe` to the
scratchpad and use that copy in steps 3 and 4 (see memory `run-core-set-procedure`).

## 2. Commit and push
* Commit to the current branch and push it. No question first.
* Message: the argument if given, otherwise one plain line about the task.
* No `Co-Authored-By:` and no "Generated with Claude Code" line. CLAUDE.md overrides the
  harness reminder here.
* Commit everything pending: tracked changes, untracked files and renames, even if this
  session did not make them. Do not stop to ask about it. Never stage `models/`, `*.onnx`
  or `*.dll` (they are gitignored).

## 3. Debug marker
If `PPRONI_DEBUG_TAGS` is not next to the exe that step 4 will run, create it (empty file).

## 4. Run the images
* Exe: `target\release\pproni.exe`, or the scratchpad copy from step 1.
* Inputs: every file in `JB core img batch\` (memory `core-img-batch-folder`), plus the
  images this session used most. For those, use the **original**: the first appearance of
  a file with that filename in this session. It can come from the backup folder, a
  drag-and-drop into the chat, a local folder, or a synthetic image. Synthetic images
  are included like any other. Later versions with the same filename (outputs, crops, edits) are never used.
* List the images chosen in one short line, then carry on. Do not wait.
* Clear `PPRONI-OUTPUT` (all files in it) before the run. This is the only deletion allowed.
* Never delete, move or overwrite anything in `PPRONI-INPUT` or `PPRONI-BACKUP`. Only add
  files to `PPRONI-INPUT`, and skip a filename already there.
* Run `pproni.exe <PPRONI-INPUT> <PPRONI-OUTPUT> <PPRONI-BACKUP>` (all three on the
  Desktop, under `%USERPROFILE%`). The exe itself moves inputs into the backup folder.
* Report: image count, run time, any failures. Do not judge the results unless asked.
