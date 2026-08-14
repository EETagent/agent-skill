#[cfg(unix)]
#[test]
fn serializes_non_utf8_paths_as_documented_lossy_strings() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    use agent_skill::model::SkillChange;

    let non_utf8 = PathBuf::from(OsString::from_vec(vec![b'a', 0xff, b'b']));
    let value = serde_json::to_value(SkillChange {
        skill: "demo".to_owned(),
        from: non_utf8.clone(),
        to: non_utf8,
    })
    .expect("non-UTF-8 paths should not fail JSON serialization");

    assert_eq!(value["from"], "a\u{fffd}b");
    assert_eq!(value["to"], "a\u{fffd}b");
}
