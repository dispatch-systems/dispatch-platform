//! The image of someone's initials Slack shows their messages under.
use super::*;

#[test]
fn samples() {
    for letters in ["JW", "MF", "KB", "QR", "SG", "AZ", "O", "XY", "DC", "TN", "WM", "IJ", "PL", "EH", "UV", "BN"] {
        std::fs::write(format!("/tmp/dispatch-communications-engine/initials-{letters}.png"), &*image(&format!("{letters}.png")).unwrap()).unwrap();
    }
}
