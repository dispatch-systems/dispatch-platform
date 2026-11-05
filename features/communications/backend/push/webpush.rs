//! Web Push's own part: the server's key pair that its pushes are signed with (VAPID, RFC 8292),
//! and each push's message encrypted for the browser it goes to (RFC 8291, `aes128gcm`, RFC 8188),
//! on RustCrypto's P-256, HKDF and AES-GCM, as WebAuthn already has them built.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use dispatch_core::{
    Error, Result,
    foundation::{config::Config, crypto, observability},
};
use hkdf::Hkdf;
use p256::{
    PublicKey, SecretKey,
    ecdsa::{Signature, SigningKey, signature::Signer},
};
use sha2::Sha256;
use std::sync::OnceLock;

/// The variable the server's private key is read from: its 32 bytes in base64url, as
/// `npx web-push generate-vapid-keys` prints a private key. Its public key follows from it.
pub const KEY: &str = "VAPID_PRIVATE_KEY";
/// How long a push's signature holds, in seconds: within the day push services allow.
const SIGNED_FOR: i64 = 12 * 60 * 60;
/// The size of the one record a push is, as its header says: larger than any message it carries.
const RECORD: u32 = 4096;

/// The server's key pair, and who it says it is: its own address.
pub struct Vapid {
    signing: SigningKey,
    /// Its public key, uncompressed, in base64url: what browsers subscribe with.
    pub public: String,
    subject: String,
}
impl Vapid {
    fn new(secret: &SecretKey, subject: &str) -> Self {
        Self {
            signing: SigningKey::from(secret),
            public: B64.encode(secret.public_key().to_sec1_bytes()),
            subject: subject.to_owned(),
        }
    }
    /// What a push to `endpoint` is authorized with, signed at `now` (seconds since 1970).
    pub fn authorization(&self, endpoint: &url::Url, now: i64) -> String {
        let header = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = B64.encode(
            serde_json::json!({
                "aud": endpoint.origin().ascii_serialization(),
                "exp": now + SIGNED_FOR,
                "sub": self.subject,
            })
            .to_string(),
        );
        let signed = format!("{header}.{claims}");
        let signature: Signature = self.signing.sign(signed.as_bytes());
        format!(
            "vapid t={signed}.{}, k={}",
            B64.encode(signature.to_bytes()),
            self.public
        )
    }
}

/// The server's key pair, read once: the one its settings hold (`VAPID_PRIVATE_KEY`), else in
/// fixture mode one made for the server's run, else none, and nothing is pushed. One that isn't a
/// key is logged, and nothing is pushed.
pub fn vapid(config: &Config) -> Option<&'static Vapid> {
    static VAPID: OnceLock<Option<Vapid>> = OnceLock::new();
    VAPID
        .get_or_init(|| {
            let secret = match config.setting(KEY) {
                Some(key) => {
                    let secret = B64
                        .decode(key.trim())
                        .ok()
                        .and_then(|bytes| SecretKey::from_slice(&bytes).ok());
                    if secret.is_none() {
                        observability::event(
                            "error",
                            "communications_push_key_invalid",
                            serde_json::json!({"variable": config.setting_variable(KEY)}),
                        );
                    }
                    secret?
                }
                None if config.fixture => secret().ok()?,
                None => return None,
            };
            Some(Vapid::new(&secret, &config.origin))
        })
        .as_ref()
}

/// A new private key, at random.
fn secret() -> Result<SecretKey> {
    loop {
        // Nearly every 32 bytes is one; the rest, past the curve's order, are tried again.
        if let Ok(secret) = SecretKey::from_slice(&crypto::random::<32>()?) {
            return Ok(secret);
        }
    }
}

/// `message` encrypted for the browser whose keys are `p256dh` and `auth`, as the body of a push
/// to it: under a key pair and salt made for it alone.
pub fn encrypt(message: &[u8], p256dh: &str, auth: &str) -> Result<Vec<u8>> {
    encrypt_with(message, p256dh, auth, &secret()?, &crypto::random::<16>()?)
}
/// `message` encrypted as `encrypt` does, with the server's key pair for it `ephemeral` and its
/// `salt` given.
fn encrypt_with(
    message: &[u8],
    p256dh: &str,
    auth: &str,
    ephemeral: &SecretKey,
    salt: &[u8; 16],
) -> Result<Vec<u8>> {
    use aes_gcm::{Aes128Gcm, KeyInit, Nonce, aead::Aead};
    let invalid = || Error::new("communications_push_subscription_invalid", 400);
    let browser = B64.decode(p256dh).map_err(|_| invalid())?;
    let secret = B64.decode(auth).map_err(|_| invalid())?;
    let theirs = PublicKey::from_sec1_bytes(&browser).map_err(|_| invalid())?;
    let ours = ephemeral.public_key().to_sec1_bytes();
    let shared = p256::ecdh::diffie_hellman(ephemeral.to_nonzero_scalar(), theirs.as_affine());
    // The key both sides reach, from the shared secret and the browser's own secret.
    let info = [b"WebPush: info\0".as_slice(), &browser, &ours].concat();
    let mut ikm = [0; 32];
    Hkdf::<Sha256>::new(Some(&secret), shared.raw_secret_bytes())
        .expand(&info, &mut ikm)
        .map_err(|_| invalid())?;
    let content = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let (mut key, mut nonce) = ([0; 16], [0; 12]);
    content
        .expand(b"Content-Encoding: aes128gcm\0", &mut key)
        .and_then(|()| content.expand(b"Content-Encoding: nonce\0", &mut nonce))
        .map_err(|_| invalid())?;
    // One record, the last: the message and its delimiter.
    let padded = [message, &[2]].concat();
    let sealed = Aes128Gcm::new_from_slice(&key)
        .map_err(|_| invalid())?
        .encrypt(&Nonce::from(nonce), padded.as_slice())
        .map_err(|_| Error::new("encryption_failed", 500))?;
    let id = u8::try_from(ours.len()).map_err(|_| invalid())?;
    Ok([
        salt.as_slice(),
        &RECORD.to_be_bytes(),
        &[id],
        &ours,
        &sealed,
    ]
    .concat())
}

/// Whether `p256dh` and `auth` are a browser's keys: a point on P-256 and a 16-byte secret.
pub fn keys_valid(p256dh: &str, auth: &str) -> bool {
    B64.decode(p256dh)
        .is_ok_and(|key| key.len() == 65 && PublicKey::from_sec1_bytes(&key).is_ok())
        && B64.decode(auth).is_ok_and(|secret| secret.len() == 16)
}

#[cfg(test)]
#[path = "../../tests/backend/push/webpush.rs"]
mod tests;
