//! `embedded/` holds the copies of the repo's examples and primitives that `src/harness.rs`
//! compiles in, so the crate can be packaged (a tarball contains nothing outside this directory).
//! The repo's own files stay the ones people edit; this test fails when a copy drifts.
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
fn every_embedded_file_equals_the_repo_original() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let embedded = here.join("embedded");
    let mut files = Vec::new();
    walk(&embedded, &mut files);
    assert!(!files.is_empty());
    for f in files {
        let rel = f.strip_prefix(&embedded).unwrap();
        let original = here.parent().unwrap().join(rel);
        assert_eq!(
            std::fs::read(&f).unwrap(),
            std::fs::read(&original).unwrap_or_else(|_| panic!("{} is gone", original.display())),
            "{} differs from {}: copy it again",
            f.display(),
            original.display()
        );
    }
}
