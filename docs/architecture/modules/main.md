# main

[Up: module pages](TOC-module-architecture.md)

Starts the `app` modules: [2. Modules: `app`](../02-modules-app.md)

## main.rs

### Command line
* **No arguments:** The desktop flow.
  * Watches `PPRONI-INPUT`
  * Writes to `PPRONI-OUTPUT`
  * Moves the originals of processed images to `PPRONI-BACKUP`

* **`<input_dir> <output_dir> [backup_dir]`:** a plain batch, used for development and testing on
  non-Windows hosts. Without the third argument the backup folder is `PPRONI-BACKUP` beside the
  output folder.
* **`--enable-headcuts`**, anywhere in the arguments, turns headcutting on for that run. The desktop
  flow asks instead.
