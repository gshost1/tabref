use super::{read_bounded, write_atomic};

fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tabref-private-file-{name}-{}", std::process::id()))
}

#[test]
fn an_atomic_write_creates_parents_and_replaces_contents() {
    let dir = scratch("write");
    let path = dir.join("nested").join("state.json");
    write_atomic(&path, b"first").unwrap();
    write_atomic(&path, b"second").unwrap();
    assert_eq!(read_bounded(&path, 64).unwrap(), b"second");
    let leftovers = std::fs::read_dir(path.parent().unwrap()).unwrap().count();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(leftovers, 1);
}

#[test]
fn a_file_over_the_limit_is_refused() {
    let dir = scratch("bounded");
    let path = dir.join("big");
    write_atomic(&path, &[b'x'; 65]).unwrap();
    let error = read_bounded(&path, 64).unwrap_err();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}
