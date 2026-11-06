//! The object store: each request signed as S3 checks it; every key the server's own, of a DSP
//! it checked, so one DSP never reads or lets go of another's; listing every page; an upload
//! going on as long as it sends; a download going on however long its reader waits between
//! reads, and given up once R2 stops answering or sending; and a bucket told from another by
//! where it answers and its name.
use super::*;
use crate::testing::objects::Fake;

const DOOR: &str = "8d3a1c0f8f2e4a8b9c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b";
const DOCK: &str = "1f2e3d4c5b6a79880716253443526170f1e2d3c4b5a69788796a5b4c3d2e1f00";

fn dsp(n: u8) -> String {
    format!("dsp_{}", format!("{n:x}").repeat(32))
}
/// `bytes`, in a file of their own.
fn file(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("bytes");
    std::fs::write(&path, bytes).unwrap();
    (folder, path)
}
async fn fetched(objects: &Objects, dsp: &str, sha256: &str) -> Option<Vec<u8>> {
    let fetched = objects.get(dsp, "phones", sha256).await.unwrap()?;
    let chunks: Vec<_> = fetched.chunks.collect().await;
    Some(
        chunks
            .into_iter()
            .flat_map(|chunk| chunk.unwrap())
            .collect(),
    )
}

#[test]
fn a_request_is_signed_as_s3_checks_it() {
    // AWS's own example of a signed GET Bucket (List Objects), from its Signature Version 4
    // documentation.
    let signing = Signing {
        method: "GET",
        host: "examplebucket.s3.amazonaws.com",
        path: "/",
        query: &canonical_query(&[("prefix", "J"), ("max-keys", "2")]),
        payload: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        at: "20130524T000000Z",
    };
    assert_eq!(
        signing.authorization(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "us-east-1"
        ),
        "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request,\
         SignedHeaders=host;x-amz-content-sha256;x-amz-date,\
         Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
    );
}

#[test]
fn a_bucket_is_told_by_where_it_answers_and_its_name_never_its_key() {
    let store = ObjectStore {
        endpoint: "https://account.r2.cloudflarestorage.com".into(),
        bucket: "pictures".into(),
        access_key_id: "key".into(),
        secret_access_key: "secret".into(),
        only: false,
    };
    let identity = |store: &ObjectStore| Objects::new(store).unwrap().identity();
    let rekeyed = ObjectStore {
        access_key_id: "another key".into(),
        secret_access_key: "another secret".into(),
        only: true,
        ..store.clone()
    };
    assert_eq!(identity(&rekeyed), identity(&store));
    let others = [
        ObjectStore {
            bucket: "files".into(),
            ..store.clone()
        },
        ObjectStore {
            endpoint: "https://other.r2.cloudflarestorage.com".into(),
            ..store.clone()
        },
    ];
    for other in others {
        assert_ne!(identity(&other), identity(&store));
    }
}

#[test]
fn a_key_is_built_only_of_a_dsp_an_owner_and_bytes_checked() {
    assert_eq!(
        key(&dsp(1), "phones", DOOR).unwrap(),
        format!("dsps/{}/phones/{DOOR}", dsp(1))
    );
    let other = format!("../{}/phones/{DOOR}", dsp(2));
    for (dsp, owner, sha256) in [
        ("dsp_1", "phones", DOOR),
        (&format!("{}/..", dsp(1)), "phones", DOOR),
        (&dsp(1), "phones/..", DOOR),
        (&dsp(1), "", DOOR),
        (&dsp(1), "phones", &other),
        (&dsp(1), "phones", &DOOR.to_uppercase()),
        (&dsp(1), "phones", &DOOR[1..]),
    ] {
        assert!(key(dsp, owner, sha256).is_err(), "{dsp} {owner} {sha256}");
    }
}

#[tokio::test]
async fn a_dsp_reads_and_lets_go_of_its_own_alone() {
    let fake = Fake::start().await;
    let objects = Objects::new(&fake.store()).unwrap();
    let (one, two) = (dsp(1), dsp(2));
    let (_door, door) = file(b"door");
    objects.put(&one, "phones", DOOR, &door).await.unwrap();
    assert_eq!(fetched(&objects, &one, DOOR).await.unwrap(), b"door");
    // Another DSP asking for the same bytes asks under its own key: it has none.
    assert_eq!(fetched(&objects, &two, DOOR).await, None);
    objects.delete(&two, "phones", DOOR).await.unwrap();
    assert_eq!(fetched(&objects, &one, DOOR).await.unwrap(), b"door");
    assert_eq!(
        objects.listed(&two, "phones").await.unwrap(),
        Vec::<String>::new()
    );
    objects.delete(&one, "phones", DOOR).await.unwrap();
    assert_eq!(fetched(&objects, &one, DOOR).await, None);
}

#[tokio::test]
async fn deleting_a_dsp_lets_go_of_every_page_of_its_own_and_nothing_else() {
    let fake = Fake::start().await;
    let objects = Objects::new(&fake.store()).unwrap();
    let (one, two) = (dsp(1), dsp(2));
    let (_bytes, path) = file(b"door");
    // More than a page of the first DSP's, of two owners, and one of the second's.
    for sha256 in [DOOR, DOCK] {
        for owner in ["phones", "communications"] {
            objects.put(&one, owner, sha256, &path).await.unwrap();
        }
    }
    objects.put(&two, "phones", DOOR, &path).await.unwrap();
    let mut listed = objects.listed(&one, "phones").await.unwrap();
    listed.sort();
    assert_eq!(listed, [DOCK, DOOR]);
    objects.delete_dsp(&one).await.unwrap();
    assert_eq!(fake.keys(), [format!("dsps/{two}/phones/{DOOR}")]);
}

#[tokio::test]
async fn an_upload_goes_on_past_the_wait_for_an_answer_while_it_sends() {
    let fake = Fake::start().await;
    let objects = Objects::new(&fake.store()).unwrap();
    // Connected before the clock stops, as connecting waits on the network and not on it: the
    // upload after goes on the same connection.
    let (_door, door) = file(b"door");
    objects.put(&dsp(1), "phones", DOOR, &door).await.unwrap();
    tokio::time::pause();
    // A slow link: each part sent well within the wait, all of them taking twice as long.
    let key = key(&dsp(1), "phones", DOCK).unwrap();
    let chunks = stream::iter([&b"do"[..], b"c", b"k"]).then(|chunk| async move {
        tokio::time::sleep(STALLED * 2 / 3).await;
        Ok(Bytes::from_static(chunk))
    });
    objects.upload(&key, 4, chunks).await.unwrap();
    assert_eq!(fake.object(&key).unwrap(), b"dock");
}

#[tokio::test]
async fn a_download_goes_on_however_long_its_reader_waits_between_reads() {
    let fake = Fake::start().await;
    let objects = Objects::new(&fake.store()).unwrap();
    let key = key(&dsp(1), "phones", DOOR).unwrap();
    fake.put(&key, b"door");
    let held = fake.hold_after(&key, 2);
    let mut fetched = objects.get(&dsp(1), "phones", DOOR).await.unwrap().unwrap();
    assert_eq!(fetched.chunks.next().await.unwrap().unwrap(), "do");
    // R2 sends the rest while the reader waits longer than R2 may go without answering, as a
    // download to a slow phone does.
    tokio::time::pause();
    held.go();
    tokio::time::sleep(STALLED * 2).await;
    let rest: Vec<_> = fetched.chunks.collect().await;
    let rest: Vec<u8> = rest.into_iter().flat_map(|chunk| chunk.unwrap()).collect();
    assert_eq!(rest, b"or");
}

#[tokio::test]
async fn a_download_is_given_up_once_r2_stops_sending() {
    let fake = Fake::start().await;
    let objects = Objects::new(&fake.store()).unwrap();
    let key = key(&dsp(1), "phones", DOOR).unwrap();
    fake.put(&key, b"door");
    let _held = fake.hold_after(&key, 2);
    let mut fetched = objects.get(&dsp(1), "phones", DOOR).await.unwrap().unwrap();
    assert_eq!(fetched.chunks.next().await.unwrap().unwrap(), "do");
    // The rest never comes.
    tokio::time::pause();
    let stalled = fetched.chunks.next().await.unwrap().unwrap_err();
    assert_eq!(stalled.kind(), std::io::ErrorKind::TimedOut);
}

#[tokio::test]
async fn a_download_is_given_up_once_r2_stops_answering() {
    let fake = Fake::start().await;
    let objects = Objects::new(&fake.store()).unwrap();
    let (door, dock) = (
        key(&dsp(1), "phones", DOOR).unwrap(),
        key(&dsp(1), "phones", DOCK).unwrap(),
    );
    fake.put(&door, b"door");
    fake.put(&dock, b"dock");
    // Connected before the clock stops, as connecting waits on the network and not on it: the
    // download after goes on the same connection.
    assert_eq!(fetched(&objects, &dsp(1), DOCK).await.unwrap(), b"dock");
    let _held = fake.hold(&door);
    tokio::time::pause();
    let unanswered = objects.get(&dsp(1), "phones", DOOR).await;
    assert_eq!(unanswered.err().unwrap().code, "object_store_unavailable");
}
