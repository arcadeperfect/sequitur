//! End-to-end tests for the core round-trip: parse -> group -> plan -> execute.

use sequitur::{
    convert_padding_to_hashes, Components, DirEntry, EntryKind, FileOperation, FileSequence, Item,
    OperationPlan, ParseResult,
};
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

#[test]
fn from_paths_buckets_by_parent_directory() {
    // Same prefix/extension in two different folders must stay distinct, and
    // each item should carry its own directory.
    let paths = [
        PathBuf::from("/a/render_001.exr"),
        PathBuf::from("/a/render_002.exr"),
        PathBuf::from("/b/render_001.exr"),
        PathBuf::from("/b/render_002.exr"),
        PathBuf::from("/b/notes.txt"), // no frame -> rogue
    ];
    let result = FileSequence::from_paths(&paths, 2);

    assert_eq!(result.sequences.len(), 2);
    // Buckets are processed in directory order, so /a precedes /b.
    assert_eq!(result.sequences[0].directory().unwrap(), Some(&PathBuf::from("/a")));
    assert_eq!(result.sequences[1].directory().unwrap(), Some(&PathBuf::from("/b")));
    assert_eq!(result.rogues, vec![PathBuf::from("/b/notes.txt")]);
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

#[test]
fn schedule_orders_dependent_renames() {
    // b -> c must run before a -> b, otherwise a -> b clobbers the source of
    // b -> c. The plan is authored in the unsafe order on purpose.
    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: PathBuf::from("/x/a"),
        destination: PathBuf::from("/x/b"),
    });
    plan.push(FileOperation::Rename {
        source: PathBuf::from("/x/b"),
        destination: PathBuf::from("/x/c"),
    });

    let schedule = plan.schedule().unwrap();
    assert_eq!(schedule.len(), 2);
    // No temp hop needed; just a reordering.
    assert_eq!(schedule[0].source(), PathBuf::from("/x/b"));
    assert_eq!(schedule[1].source(), PathBuf::from("/x/a"));
}

#[test]
fn schedule_breaks_a_swap_with_a_temp_hop() {
    // a <-> b is a 2-cycle; it cannot be ordered without a temporary.
    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: PathBuf::from("/x/a"),
        destination: PathBuf::from("/x/b"),
    });
    plan.push(FileOperation::Rename {
        source: PathBuf::from("/x/b"),
        destination: PathBuf::from("/x/a"),
    });

    let schedule = plan.schedule().unwrap();
    // Three steps: hop one out to a temp, move the other, move the temp in.
    assert_eq!(schedule.len(), 3);
    // First step renames a live source into a temp path.
    let first_dest = schedule[0].destination().unwrap().to_path_buf();
    assert!(first_dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap()
        .contains("sequitur-tmp"));
    // Last step moves that temp back into place.
    assert_eq!(schedule[2].source(), first_dest);
}

#[test]
fn commit_swaps_two_files_without_data_loss() {
    let dir = temp_dir("swap");
    fs::write(dir.join("a.txt"), b"A").unwrap();
    fs::write(dir.join("b.txt"), b"B").unwrap();

    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: dir.join("a.txt"),
        destination: dir.join("b.txt"),
    });
    plan.push(FileOperation::Rename {
        source: dir.join("b.txt"),
        destination: dir.join("a.txt"),
    });

    // Both destinations exist up front, but each is freed by the other op,
    // so the swap is allowed without force.
    let result = plan.commit(false).unwrap();
    assert!(result.success());

    assert_eq!(fs::read(dir.join("a.txt")).unwrap(), b"B");
    assert_eq!(fs::read(dir.join("b.txt")).unwrap(), b"A");

    // No stray temp files left behind.
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("sequitur-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn commit_reports_conflict_against_unfreed_destination() {
    let dir = temp_dir("commit_conflict");
    fs::write(dir.join("x.txt"), b"x").unwrap();
    fs::write(dir.join("y.txt"), b"y").unwrap();

    // x -> y where y exists and nothing frees it: a real conflict.
    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: dir.join("x.txt"),
        destination: dir.join("y.txt"),
    });

    let err = plan.commit(false).unwrap_err();
    assert!(matches!(err, sequitur::SequenceError::Conflict(_)));
    // force overrides it.
    assert!(plan.commit(true).unwrap().success());
    assert_eq!(fs::read(dir.join("y.txt")).unwrap(), b"x");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn commit_allows_case_only_rename_on_case_insensitive_fs() {
    // On a case-insensitive volume (macOS default) `RENDER.txt` "exists" the
    // moment `render.txt` does — but renaming one to the other clobbers
    // nothing. The pre-flight must not flag it as a conflict.
    let dir = temp_dir("caserename");
    fs::write(dir.join("render.txt"), b"R").unwrap();

    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: dir.join("render.txt"),
        destination: dir.join("RENDER.txt"),
    });

    // Detect the volume's case sensitivity; only assert the no-conflict
    // behaviour where it actually applies.
    let case_insensitive = dir.join("RENDER.txt").exists();
    let result = plan.commit(false);
    if case_insensitive {
        assert!(result.unwrap().success(), "case-only rename wrongly blocked");
        assert!(dir.join("RENDER.txt").exists());
    } else {
        // Case-sensitive: RENDER.txt genuinely does not exist, so it also
        // commits cleanly (no conflict either way).
        assert!(result.unwrap().success());
    }

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn commit_rotates_three_files() {
    let dir = temp_dir("rotate");
    fs::write(dir.join("a"), b"A").unwrap();
    fs::write(dir.join("b"), b"B").unwrap();
    fs::write(dir.join("c"), b"C").unwrap();

    // a -> b -> c -> a (3-cycle).
    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: dir.join("a"),
        destination: dir.join("b"),
    });
    plan.push(FileOperation::Rename {
        source: dir.join("b"),
        destination: dir.join("c"),
    });
    plan.push(FileOperation::Rename {
        source: dir.join("c"),
        destination: dir.join("a"),
    });

    assert!(plan.commit(false).unwrap().success());
    assert_eq!(fs::read(dir.join("b")).unwrap(), b"A");
    assert_eq!(fs::read(dir.join("c")).unwrap(), b"B");
    assert_eq!(fs::read(dir.join("a")).unwrap(), b"C");

    let leftovers = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("sequitur-tmp"))
        .count();
    assert_eq!(leftovers, 0);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn validate_is_pure_and_honest_about_dependent_ops() {
    use std::collections::HashSet;

    // The projected world contains a, b and c. None of these paths are touched
    // on disk — validate must not consult the filesystem.
    let projected: HashSet<PathBuf> =
        ["/v/a", "/v/b", "/v/c"].iter().map(PathBuf::from).collect();

    // A clean swap: each destination exists in the projected set but is freed
    // by the partner op, so there is no conflict.
    let mut swap = OperationPlan::new();
    swap.push(FileOperation::Rename {
        source: PathBuf::from("/v/a"),
        destination: PathBuf::from("/v/b"),
    });
    swap.push(FileOperation::Rename {
        source: PathBuf::from("/v/b"),
        destination: PathBuf::from("/v/a"),
    });
    assert!(swap.validate(&projected).is_ok());

    // Renaming onto c, which exists in the projected set and is freed by no
    // one, is a genuine conflict.
    let mut clobber = OperationPlan::new();
    clobber.push(FileOperation::Rename {
        source: PathBuf::from("/v/a"),
        destination: PathBuf::from("/v/c"),
    });
    let err = clobber.validate(&projected).unwrap_err();
    match err {
        sequitur::SequenceError::Conflict(paths) => {
            assert_eq!(paths, vec![PathBuf::from("/v/c")]);
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
}

#[test]
fn validate_flags_two_ops_targeting_one_destination() {
    use std::collections::HashSet;

    // Empty projected world: the only conflict is internal to the plan.
    let projected: HashSet<PathBuf> = HashSet::new();

    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: PathBuf::from("/v/a"),
        destination: PathBuf::from("/v/x"),
    });
    plan.push(FileOperation::Rename {
        source: PathBuf::from("/v/b"),
        destination: PathBuf::from("/v/x"),
    });

    let err = plan.validate(&projected).unwrap_err();
    assert!(matches!(err, sequitur::SequenceError::Conflict(_)));
}

#[test]
fn commit_observed_reports_every_scheduled_step() {
    use std::ops::ControlFlow;

    let dir = temp_dir("observe");
    fs::write(dir.join("a"), b"A").unwrap();
    fs::write(dir.join("b"), b"B").unwrap();

    // A swap, so the schedule has 3 steps (one synthetic temp hop).
    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: dir.join("a"),
        destination: dir.join("b"),
    });
    plan.push(FileOperation::Rename {
        source: dir.join("b"),
        destination: dir.join("a"),
    });

    let mut seen = Vec::new();
    let result = plan
        .commit_observed(false, |p| {
            assert_eq!(p.total, 3);
            seen.push(p.index);
            ControlFlow::Continue(())
        })
        .unwrap();

    assert!(result.success());
    assert_eq!(seen, vec![0, 1, 2]);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn commit_observed_cancellation_unwinds_temp_cycle() {
    use std::ops::ControlFlow;

    let dir = temp_dir("cancel");
    fs::write(dir.join("a"), b"A").unwrap();
    fs::write(dir.join("b"), b"B").unwrap();

    // Cancel mid-swap: stop just before the final step that moves the temp
    // back into place. The in-flight temp must be unwound, restoring a and b.
    let mut plan = OperationPlan::new();
    plan.push(FileOperation::Rename {
        source: dir.join("a"),
        destination: dir.join("b"),
    });
    plan.push(FileOperation::Rename {
        source: dir.join("b"),
        destination: dir.join("a"),
    });

    let result = plan
        .commit_observed(false, |p| {
            if p.index == 2 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap();

    // Cancellation is not a failure.
    assert!(result.success());

    // Original files restored, nothing applied, no temp left behind.
    assert_eq!(fs::read(dir.join("a")).unwrap(), b"A");
    assert_eq!(fs::read(dir.join("b")).unwrap(), b"B");
    let leftovers = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("sequitur-tmp"))
        .count();
    assert_eq!(leftovers, 0);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn directory_listing_classifies_all_entries() {
    let dir = temp_dir("listing");
    // A real two-frame sequence.
    fs::write(dir.join("shot_001.exr"), b"x").unwrap();
    fs::write(dir.join("shot_002.exr"), b"x").unwrap();
    // A lone frame below min_frames must still appear (as a loose file).
    fs::write(dir.join("lone_010.exr"), b"x").unwrap();
    // An unparseable loose file, a hidden file, and a subdirectory.
    fs::write(dir.join("notes.txt"), b"x").unwrap();
    fs::write(dir.join(".hidden"), b"x").unwrap();
    fs::create_dir(dir.join("subdir")).unwrap();

    let listing = FileSequence::parse_directory(&dir, 2).unwrap();

    // Exactly one sequence: shot_###, holding only its two frames.
    assert_eq!(listing.sequences.len(), 1);
    assert_eq!(listing.sequences[0].len(), 2);

    let find = |name: &str| -> &DirEntry {
        listing
            .entries
            .iter()
            .find(|e| e.path.ends_with(name))
            .unwrap_or_else(|| panic!("missing entry {name}"))
    };

    // Sub-threshold frame and unparseable file are surfaced as loose files.
    assert_eq!(find("lone_010.exr").kind, EntryKind::File);
    assert_eq!(find("notes.txt").kind, EntryKind::File);
    // Subdirectory is classified, not dropped.
    assert_eq!(find("subdir").kind, EntryKind::Directory);
    // Hidden entry is surfaced and flagged.
    assert!(find(".hidden").hidden);
    // None of the sequence's own frames leak into entries.
    assert!(!listing.entries.iter().any(|e| e.path.ends_with("shot_001.exr")));
    // Entries are sorted by path.
    let paths: Vec<_> = listing.entries.iter().map(|e| e.path.clone()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);

    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn directory_listing_reports_symlinks() {
    let dir = temp_dir("symlink");
    fs::write(dir.join("target.txt"), b"x").unwrap();
    std::os::unix::fs::symlink(dir.join("target.txt"), dir.join("link.txt")).unwrap();

    let listing = FileSequence::parse_directory(&dir, 2).unwrap();
    let link = listing.entries.iter().find(|e| e.path.ends_with("link.txt")).unwrap();
    assert_eq!(link.kind, EntryKind::Symlink);

    fs::remove_dir_all(&dir).unwrap();
}

// ---------------------------------------------------------------------------
// Robust parsing: duplicate-frame splitting + rogue tracking
// ---------------------------------------------------------------------------

#[test]
fn splits_anomalous_padding_into_separate_sequences() {
    // Frames 3 and 4 appear at both padding 3 (dominant) and padding 2.
    let files = [
        "render_001.exr",
        "render_002.exr",
        "render_003.exr",
        "render_004.exr",
        "render_03.exr",
        "render_04.exr",
    ];
    let seqs = FileSequence::from_filenames(&files, 2, None);
    assert_eq!(seqs.len(), 2);

    // Main sequence: the four nominally-padded frames.
    assert_eq!(seqs[0].len(), 4);
    assert_eq!(seqs[0].padding().unwrap(), 3);

    // Anomalous sequence: the two padding-2 frames.
    assert_eq!(seqs[1].len(), 2);
    assert_eq!(seqs[1].padding().unwrap(), 2);
}

#[test]
fn lone_anomalous_frame_is_dropped() {
    // Frame 3 is duplicated, but the padding-2 fragment has only one frame,
    // so it is discarded and a single clean sequence remains.
    let files = [
        "render_001.exr",
        "render_002.exr",
        "render_003.exr",
        "render_03.exr",
    ];
    let seqs = FileSequence::from_filenames(&files, 2, None);
    assert_eq!(seqs.len(), 1);
    assert_eq!(seqs[0].len(), 3);
    assert_eq!(seqs[0].padding().unwrap(), 3);
}

#[test]
fn find_duplicate_frames_orders_nominal_first() {
    let files = ["a_001.exr", "a_002.exr", "a_003.exr", "a_03.exr"];
    // Build the raw group via parse to keep all four items together, then
    // inspect duplicates on a sequence that still holds the clash.
    let items: Vec<Item> = files
        .iter()
        .map(|f| Item::from_filename(f, None).unwrap())
        .collect();
    let seq = FileSequence::new(items).unwrap();
    let dups = seq.find_duplicate_frames();
    let frame3 = &dups[&3];
    assert_eq!(frame3.len(), 2);
    // Nominal padding (3) comes first.
    assert_eq!(frame3[0].padding(), 3);
    assert_eq!(frame3[1].padding(), 2);
}

#[test]
fn rogues_are_reported() {
    let files = ["a_001.exr", "a_002.exr", "notes.txt", "README", ".hidden"];
    let ParseResult { sequences, rogues } = FileSequence::parse_filenames(&files, 2, None);
    assert_eq!(sequences.len(), 1);
    assert_eq!(rogues.len(), 2); // dotfile excluded
    assert!(rogues.contains(&PathBuf::from("notes.txt")));
    assert!(rogues.contains(&PathBuf::from("README")));
}

#[test]
fn rogues_are_prefixed_with_directory() {
    let files = ["a_001.exr", "a_002.exr", "junk"];
    let result = FileSequence::parse_filenames(&files, 2, Some(PathBuf::from("/tmp/shots")));
    assert_eq!(result.rogues, vec![PathBuf::from("/tmp/shots/junk")]);
}

// ---------------------------------------------------------------------------
// Ergonomics: indexing, iteration, frame queries, counts
// ---------------------------------------------------------------------------

#[test]
fn indexing_and_iteration() {
    let files = ["seq_001.exr", "seq_002.exr", "seq_004.exr", "seq_005.exr"];
    let seqs = FileSequence::from_filenames(&files, 2, None);
    let seq = &seqs[0];

    // Positional indexing, frame-ordered.
    assert_eq!(seq[0].frame_number(), 1);
    assert_eq!(seq[3].frame_number(), 5);

    // iter() and IntoIterator for &FileSequence.
    let via_iter: Vec<i32> = seq.iter().map(|i| i.frame_number()).collect();
    assert_eq!(via_iter, vec![1, 2, 4, 5]);
    let via_into: Vec<i32> = (seq).into_iter().map(|i| i.frame_number()).collect();
    assert_eq!(via_into, via_iter);
}

#[test]
fn frame_queries_and_counts() {
    let files = ["seq_001.exr", "seq_002.exr", "seq_004.exr", "seq_005.exr"];
    let seq = &FileSequence::from_filenames(&files, 2, None)[0];

    assert!(seq.contains(2));
    assert!(!seq.contains(3));
    assert_eq!(seq.get_frame(4).unwrap().frame_number(), 4);
    assert!(seq.get_frame(3).is_none());

    assert_eq!(seq.actual_frame_count(), 4);
    assert_eq!(seq.frame_count(), 5); // span 1..=5 includes the missing 3
    assert_eq!(seq.missing_frames(), vec![3]);
}

#[test]
fn frames_returns_inclusive_subrange() {
    let files = ["seq_001.exr", "seq_002.exr", "seq_003.exr", "seq_004.exr"];
    let seq = &FileSequence::from_filenames(&files, 2, None)[0];

    let sub = seq.frames(2, 3);
    assert_eq!(sub.len(), 2);
    assert_eq!(sub.first_frame(), 2);
    assert_eq!(sub.last_frame(), 3);

    // Out-of-range request yields an empty (but valid) sequence.
    let empty = seq.frames(100, 200);
    assert!(empty.is_empty());
}

// ---------------------------------------------------------------------------
// Sequence-string notation (render + parse)
// ---------------------------------------------------------------------------

#[test]
fn renders_sequence_string() {
    let seq = &FileSequence::from_filenames(&["render_001.exr", "render_002.exr"], 2, None)[0];
    assert_eq!(seq.sequence_string().unwrap(), "render_###.exr");
    assert_eq!(seq.sequence_string_printf().unwrap(), "render_%03d.exr");
}

#[test]
fn parses_hash_and_printf_patterns() {
    let hash = Components::from_sequence_string("render_####.exr").unwrap();
    assert_eq!(hash.prefix.as_deref(), Some("render"));
    assert_eq!(hash.delimiter.as_deref(), Some("_"));
    assert_eq!(hash.padding, Some(4));
    assert_eq!(hash.suffix, None);
    assert_eq!(hash.extension.as_deref(), Some("exr"));
    assert_eq!(hash.frame_number, None);

    // printf form parses identically.
    let printf = Components::from_sequence_string("render_%04d.exr").unwrap();
    assert_eq!(printf, hash);
}

#[test]
fn parses_pattern_with_suffix() {
    let c = Components::from_sequence_string("frame####_final.jpg").unwrap();
    assert_eq!(c.prefix.as_deref(), Some("frame"));
    assert_eq!(c.delimiter, None);
    assert_eq!(c.padding, Some(4));
    assert_eq!(c.suffix.as_deref(), Some("_final"));
    assert_eq!(c.extension.as_deref(), Some("jpg"));
}

#[test]
fn pattern_without_placeholder_is_none() {
    assert!(Components::from_sequence_string("render.exr").is_none());
}

#[test]
fn sequence_string_round_trips_through_parse() {
    let seq = &FileSequence::from_filenames(&["shot_0001.exr", "shot_0002.exr"], 2, None)[0];
    let s = seq.sequence_string().unwrap();
    let c = Components::from_sequence_string(&s).unwrap();
    assert_eq!(c.prefix.as_deref(), Some("shot"));
    assert_eq!(c.delimiter.as_deref(), Some("_"));
    assert_eq!(c.padding, Some(4));
    assert_eq!(c.extension.as_deref(), Some("exr"));
}

#[test]
fn printf_to_hash_conversion() {
    assert_eq!(convert_padding_to_hashes("render_%04d.exr"), "render_####.exr");
    assert_eq!(convert_padding_to_hashes("%d"), "#");
    assert_eq!(convert_padding_to_hashes("a_%4d_b"), "a_####_b");
    // A bare percent that isn't a printf token is left alone.
    assert_eq!(convert_padding_to_hashes("50% done"), "50% done");
}

// ---------------------------------------------------------------------------
// FileSequence verbs: copy / offset_frames / with_padding
// ---------------------------------------------------------------------------

#[test]
fn copy_renames_components_per_item() {
    let seq = &FileSequence::from_filenames(&["render_001.exr", "render_002.exr"], 2, None)[0];
    let planned = seq.copy(Some(Components::new().prefix("bak")), None);

    let names: Vec<String> = planned.proposed.iter().map(|i| i.filename()).collect();
    assert_eq!(names, vec!["bak_001.exr", "bak_002.exr"]);
    assert_eq!(planned.plan.len(), 2);
    assert!(planned
        .plan
        .operations()
        .iter()
        .all(|op| matches!(op, FileOperation::Copy { .. })));
}

#[test]
fn offset_frames_shifts_and_keeps_padding() {
    let seq = &FileSequence::from_filenames(
        &["render_001.exr", "render_002.exr", "render_003.exr"],
        2,
        None,
    )[0];
    let planned = seq.offset_frames(10, None).unwrap();
    assert_eq!(planned.proposed.existing_frames(), vec![11, 12, 13]);
    assert_eq!(planned.plan.len(), 3);
    let names: Vec<String> = planned.proposed.iter().map(|i| i.filename()).collect();
    assert_eq!(names, vec!["render_011.exr", "render_012.exr", "render_013.exr"]);
}

#[test]
fn offset_frames_widens_padding_when_needed() {
    let seq = &FileSequence::from_filenames(&["a_8.exr", "a_9.exr"], 2, None)[0];
    let planned = seq.offset_frames(2, None).unwrap();
    let names: Vec<String> = planned.proposed.iter().map(|i| i.filename()).collect();
    assert_eq!(names, vec!["a_10.exr", "a_11.exr"]);
}

#[test]
fn offset_frames_zero_is_noop() {
    let seq = &FileSequence::from_filenames(&["a_001.exr", "a_002.exr"], 2, None)[0];
    let planned = seq.offset_frames(0, None).unwrap();
    assert!(planned.plan.is_empty());
    assert_eq!(planned.proposed.existing_frames(), vec![1, 2]);
}

#[test]
fn offset_frames_rejects_negative_result() {
    let seq = &FileSequence::from_filenames(&["a_001.exr", "a_002.exr"], 2, None)[0];
    let err = seq.offset_frames(-5, None).unwrap_err();
    assert!(matches!(err, sequitur::SequenceError::NegativeFrame(_)));
}

#[test]
fn with_padding_grows_and_respects_floor() {
    let seq = &FileSequence::from_filenames(&["render_001.exr", "render_002.exr"], 2, None)[0];
    let grown = seq.with_padding(5);
    let names: Vec<String> = grown.proposed.iter().map(|i| i.filename()).collect();
    assert_eq!(names, vec!["render_00001.exr", "render_00002.exr"]);

    // Requesting fewer digits than the largest frame needs is clamped: frame
    // 101 needs 3, so padding 1 is a no-op.
    let wide = &FileSequence::from_filenames(&["x_100.exr", "x_101.exr"], 2, None)[0];
    let shrunk = wide.with_padding(1);
    assert!(shrunk.plan.is_empty());
    assert_eq!(shrunk.proposed.iter().next().unwrap().padding(), 3);
}
