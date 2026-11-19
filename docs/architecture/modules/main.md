# main

[Up: architecture](../TOC.md)

Detail moved out of the comments of `src/main.rs`. The `app` modules it starts are
[2. Modules: `app`](../02-modules-app.md).

## main.rs

### Command line
* **No arguments:** the desktop flow. It watches the desktop `PPRONI-INPUT` folder, writes to
  `PPRONI-OUTPUT` and moves the originals to `PPRONI-BACKUP`.
* **`<input_dir> <output_dir> [backup_dir]`:** a plain batch, used for development and testing on
  non-Windows hosts. Without the third argument the backup folder is `PPRONI-BACKUP` beside the
  output folder.
* **`--enable-headcuts`**, anywhere in the arguments, turns headcutting on for that run. The desktop
  flow asks instead.
