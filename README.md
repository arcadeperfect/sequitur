# sequitur

> **Beta (0.2.x).** The core API is usable and tested, but may still change before 1.0.

A Rust library for identifying and manipulating sequences of files. Geared towards visual effects and animation pipelines, but usable with any numbered file sequences.

This is a Rust port of [pysequitur](https://github.com/arcadeperfect/pysequitur), a Python library by the same author.

## Install

```toml
[dependencies]
sequitur = "0.2"
```

Enable the optional `serde` feature to (de)serialize the public types — handy for sending sequences and operation plans across an IPC boundary (e.g. a Tauri command):

```toml
sequitur = { version = "0.2", features = ["serde"] }
```

## Features

- **File Sequence Handling**
  - Parse a directory, or a loose list of paths dropped from several folders (`from_paths`)
  - Group files into sequences; report unparseable files as rogues
  - Detect missing frames; split sequences with duplicate or inconsistent padding
  - Render sequence strings in hash (`render_####.exr`) or printf (`render_%04d.exr`) notation

- **Flexible Component System**
  - Parse filenames into components (prefix, delimiter, frame number, suffix, extension)
  - Modify individual components while preserving others

- **Sequence Operations**
  - Rename, move, copy, delete sequences
  - Offset frame numbers
  - Adjust or repair frame number padding

- **Safe by Default**
  - Operations return a plan that can be inspected before execution
  - Conflict detection prevents accidental overwrites

## Usage

```rust
use sequitur::{Components, FileSequence};
use std::path::Path;

// Discover sequences in a directory (minimum 2 frames each).
let sequences = FileSequence::from_directory(Path::new("/renders"), 2)?;

for seq in &sequences {
    println!(
        "{}  frames {}-{}",
        seq.sequence_string()?, // e.g. "render_####.exr"
        seq.first_frame(),
        seq.last_frame(),
    );
}

// Plan a rename, inspect it, then apply (false = don't overwrite).
let planned = sequences[0].rename(Components::new().prefix("shot_010"));
if !planned.plan.has_conflicts() {
    let renamed = planned.apply(false)?;
    println!("renamed to {}", renamed.sequence_string()?);
}
```

Files dropped from several folders at once can be ingested directly — they're
grouped by parent directory, and anything without a frame number comes back as
a rogue:

```rust
use sequitur::FileSequence;
use std::path::PathBuf;

let dropped: Vec<PathBuf> = /* paths from a drag-and-drop event */ vec![];
let result = FileSequence::from_paths(&dropped, 1);
// result.sequences — the sequences found
// result.rogues    — paths with no detectable frame number
```

## File Naming Convention

The library parses filenames into the following components:

```
<prefix><delimiter><frame><suffix>.<extension>
```

Example: `render_001_final.exr`
- prefix: `render`
- delimiter: `_`
- frame: `001`
- suffix: `_final`
- extension: `exr`

## License

MIT
