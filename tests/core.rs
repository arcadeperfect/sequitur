//! End-to-end tests for the core round-trip: parse -> group -> plan -> execute.

use sequitur::{Components, FileSequence, Item};
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[test]
fn parses_delimited_name() {
    let item = Item::from_filename("render_001.exr", None).unwrap();
    assert_eq!(item.prefix(), "render");
    assert_eq!(item.delimiter(), Some("_"));
    assert_eq!(item.frame_string(), "001");
    assert_eq!(item.frame_number(), 1);
    assert_eq!(item.suffix(), None);
    assert_eq!(item.extension(), "exr");
    assert_eq!(item.padding(), 3);
}

#[test]
fn parses_dot_delimiter() {
    let item = Item::from_filename("comp.001.exr", None).unwrap();
    assert_eq!(item.prefix(), "comp");
    assert_eq!(item.delimiter(), Some("."));
    assert_eq!(item.frame_string(), "001");
    assert_eq!(item.extension(), "exr");
}

#[test]
fn parses_suffix_and_no_delimiter() {
    let item = Item::from_filename("frame0001_final.jpg", None).unwrap();
    assert_eq!(item.prefix(), "frame");
    assert_eq!(item.delimiter(), None);
    assert_eq!(item.frame_string(), "0001");
    assert_eq!(item.suffix(), Some("_final"));
    assert_eq!(item.extension(), "jpg");
}

#[test]
fn multi_char_delimiter_folds_into_prefix() {
    let item = Item::from_filename("name__001.exr", None).unwrap();
    assert_eq!(item.prefix(), "name_");
    assert_eq!(item.delimiter(), Some("_"));
    assert_eq!(item.frame_string(), "001");
}

#[test]
fn frame_is_the_last_digit_run() {
    let item = Item::from_filename("shot_010_v2_0042.png", None).unwrap();
    assert_eq!(item.prefix(), "shot_010_v2");
    assert_eq!(item.delimiter(), Some("_"));
    assert_eq!(item.frame_string(), "0042");
    assert_eq!(item.extension(), "png");
}

#[test]
fn honours_known_compound_extension() {
    let item = Item::from_filename("backup_001.tar.gz", None).unwrap();
    assert_eq!(item.prefix(), "backup");
    assert_eq!(item.delimiter(), Some("_"));
    assert_eq!(item.frame_string(), "001");
    assert_eq!(item.extension(), "tar.gz");
}

#[test]
fn no_digits_is_not_an_item() {
    assert!(Item::from_filename("readme.txt", None).is_none());
}

#[test]
fn filename_round_trips() {
    let item = Item::from_filename("render_001.exr", None).unwrap();
    assert_eq!(item.filename(), "render_001.exr");
}

#[test]
fn from_path_splits_directory() {
    let item = Item::from_path(&PathBuf::from("/tmp/shots/render_001.exr")).unwrap();
    assert_eq!(item.prefix(), "render");
    assert_eq!(item.directory(), Some(&PathBuf::from("/tmp/shots")));
}

// ---------------------------------------------------------------------------
// Grouping
// ---------------------------------------------------------------------------

#[test]
fn groups_into_sequences() {
    let files = [
        "render_001.exr",
        "render_002.exr",
        "render_003.exr",
        "comp.001.exr",
        "comp.002.exr",
        "stray.txt",  // no digits -> skipped
        "solo_007.exr", // below min_frames -> dropped
        ".hidden_001.exr", // dotfile -> skipped
    ];
    let seqs = FileSequence::from_filenames(&files, 2, None);
    assert_eq!(seqs.len(), 2);

    // BTreeMap key order: ("comp", ".", ...) sorts before ("render", "_", ...)
    let comp = &seqs[0];
    assert_eq!(comp.prefix().unwrap(), "comp");
    assert_eq!(comp.len(), 2);

    let render = &seqs[1];
    assert_eq!(render.prefix().unwrap(), "render");
    assert_eq!(render.len(), 3);
    assert_eq!(render.first_frame(), 1);
    assert_eq!(render.last_frame(), 3);
    assert!(render.missing_frames().is_empty());
}

#[test]
fn missing_frames_detected() {
    let files = ["seq_001.exr", "seq_002.exr", "seq_004.exr"];
    let seqs = FileSequence::from_filenames(&files, 2, None);
    assert_eq!(seqs.len(), 1);
    assert_eq!(seqs[0].missing_frames(), vec![3]);
}

// ---------------------------------------------------------------------------
// Planning (no filesystem)
// ---------------------------------------------------------------------------

#[test]
fn rename_plan_targets_expected_paths() {
    let item = Item::from_filename("render_001.exr", Some(PathBuf::from("/tmp/shots"))).unwrap();
    let planned = item.rename(Components::new().prefix("shot"));
    assert_eq!(planned.proposed.filename(), "shot_001.exr");
    assert_eq!(planned.plan.len(), 1);
    let op = &planned.plan.operations()[0];
    assert_eq!(op.source(), PathBuf::from("/tmp/shots/render_001.exr"));
    assert_eq!(
        op.destination().unwrap(),
        PathBuf::from("/tmp/shots/shot_001.exr")
    );
}

// ---------------------------------------------------------------------------
// Execution (real filesystem, in a temp dir)
// ---------------------------------------------------------------------------

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("sequitur_test_{tag}_{}_{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn executes_a_sequence_rename_on_disk() {
    let dir = temp_dir("rename");
    for n in 1..=3 {
        fs::write(dir.join(format!("render_00{n}.exr")), b"x").unwrap();
    }

    let seqs = FileSequence::from_directory(&dir, 2).unwrap();
    assert_eq!(seqs.len(), 1);

    let planned = seqs[0].rename(Components::new().prefix("shot"));
    assert!(!planned.plan.has_conflicts());

    let new_seq = planned.apply(false).unwrap();
    assert_eq!(new_seq.len(), 3);

    for n in 1..=3 {
        assert!(dir.join(format!("shot_00{n}.exr")).exists());
        assert!(!dir.join(format!("render_00{n}.exr")).exists());
    }

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn execute_reports_conflict_without_force() {
    let dir = temp_dir("conflict");
    fs::write(dir.join("a_001.exr"), b"x").unwrap();
    fs::write(dir.join("a_002.exr"), b"x").unwrap();
    // Pre-create the destination of the first rename so it conflicts.
    fs::write(dir.join("b_001.exr"), b"x").unwrap();

    let seqs = FileSequence::from_directory(&dir, 2).unwrap();
    let a = seqs.iter().find(|s| s.prefix().unwrap() == "a").unwrap();
    let planned = a.rename(Components::new().prefix("b"));

    assert!(planned.plan.has_conflicts());
    let err = planned.plan.execute(false).unwrap_err();
    assert!(matches!(err, sequitur::SequenceError::Conflict(_)));

    fs::remove_dir_all(&dir).unwrap();
}
