//! Bytes kept by their SHA-256, as upkeep looks at them: those gone once upkeep listed them went
//! already.
use super::*;
use crate::backend::phone::support::{kept, store};

#[test]
fn bytes_gone_once_upkeep_listed_them_went_already() {
    let (_root, db, dsp) = store();
    let folder = kept(&db, &dsp);
    // Upkeep lists an upload's bytes as they arrive; it's cancelled before upkeep looks at them.
    std::fs::write(folder.join(".arriving_cancelled"), b"do").unwrap();
    let listed = std::fs::read_dir(&folder).unwrap().next().unwrap().unwrap();
    std::fs::remove_file(listed.path()).unwrap();
    assert!(!unkept(&listed, &HashSet::new(), i64::MAX).unwrap());
}
