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
  - Parse and manage frame-based file sequences
  - Support for various naming conventions and patterns
  - Handle missing or duplicate frames, inconsistent padding

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
