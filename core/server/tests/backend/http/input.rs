use super::*;
#[tokio::test]
async fn encoded_replies_preserve_json_status_and_cookies() {
    let value = json!({"rows":[{"name":"Álvaro","hours":8.5}]});
    let reply = Reply::of_status(&value, 201)
        .unwrap()
        .cookie("test=value".into())
        .into_response();
    assert_eq!(reply.status(), 201);
    assert_eq!(reply.headers()["content-type"], "application/json");
    assert_eq!(reply.headers()["set-cookie"], "test=value");
    let body = axum::body::to_bytes(reply.into_body(), 1024).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), value);
}
#[test]
fn redirects_name_where_to_go_and_carry_nothing_else() {
    let reply = Reply::redirect("https://d.example/#authorize?request=r".into()).into_response();
    assert_eq!(reply.status(), 302);
    assert_eq!(
        reply.headers()["location"],
        "https://d.example/#authorize?request=r"
    );
    assert!(reply.headers().get("content-type").is_none());
}
