use super::*;
use crate::testing;
use axum::{Json, Router, routing::get};

#[test]
fn request_events_name_only_the_final_refusing_gate() {
    const PRIVATE: &str = "private_request_and_storage_detail";
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (replies, events) = observability::recorded(|| {
        runtime.block_on(async {
            testing::install(&[], &[]);
            let (_root, store, _) = testing::bootstrapped();
            let config = store.config.clone();
            drop(store);
            let state = State::new(config).unwrap();
            let host = format!("127.0.0.1:{}", state.config.port);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let app = Router::new()
                .route(
                    "/database",
                    get(|AxumState(state): AxumState<Arc<State>>| async move {
                        match state.read(|_| Ok(())).await {
                            Ok(()) => Json(json!({"ok":true})).into_response(),
                            Err(error) => failure(error),
                        }
                    }),
                )
                .route(
                    "/noted",
                    get(|| async {
                        noted(
                            (
                                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                                Json(json!({"error":"platform_busy","message":"platform busy"})),
                            )
                                .into_response(),
                            "platform_busy",
                        )
                    }),
                )
                .route(
                    "/caused",
                    get(|| async { failure(Error::from(std::io::Error::other(PRIVATE))) }),
                )
                .route(
                    "/caught",
                    get(|| async {
                        let _discarded = Error::busy("discarded_gate");
                        Json(json!({"ok":true}))
                    }),
                )
                .with_state(state.clone())
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    pipeline,
                ));
            let (stop, stopped) = tokio::sync::oneshot::channel();
            let serving = tokio::spawn(async move {
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await
            });
            let mut held = Some(
                state
                    .db_queue
                    .clone()
                    .acquire_many_owned(state.db_queue.available_permits() as u32)
                    .await
                    .unwrap(),
            );
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let collected = async {
                let mut replies = Vec::new();
                for (index, path) in ["database", "noted", "caused", "caught", "database"]
                    .into_iter()
                    .enumerate()
                {
                    if index == 4 {
                        drop(held.take());
                    }
                    let response = client
                        .get(format!("{url}/{path}?private={PRIVATE}"))
                        .header("host", &host)
                        .header("x-private-context", PRIVATE)
                        .send()
                        .await?;
                    let id = response.headers()["x-request-id"]
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let status = response.status().as_u16();
                    let exposed = response.headers().contains_key("admission-origin")
                        || response.headers().contains_key("x-admission-origin");
                    let body = response.json::<Value>().await?;
                    replies.push((id, status, exposed, body));
                }
                Ok::<_, reqwest::Error>(replies)
            }
            .await;
            // Always release the controlled wait before shutting down, including transport errors.
            drop(held);
            state.db_queue.close();
            drop(client);
            let _ = stop.send(());
            serving.await.unwrap().unwrap();
            collected.unwrap()
        })
    });
    let requests: Vec<_> = events
        .iter()
        .filter(|event| event["event"] == "http.request")
        .collect();
    assert_eq!(requests.len(), 5);
    for (index, (id, status, exposed, body)) in replies.iter().enumerate() {
        assert_eq!(*status, [503, 503, 500, 200, 200][index]);
        assert!(!exposed);
        let matching: Vec<_> = requests
            .iter()
            .filter(|event| event["fields"]["requestId"].as_str() == Some(id.as_str()))
            .collect();
        assert_eq!(matching.len(), 1);
        let fields = &matching[0]["fields"];
        assert_eq!(fields["status"], *status);
        if index == 0 {
            assert_eq!(fields["admissionOrigin"], "db_queue_full");
            assert_eq!(
                body,
                &json!({"error":"platform_busy","message":"platform busy"})
            );
        } else {
            assert!(fields.get("admissionOrigin").is_none());
        }
        assert!(!fields.to_string().contains(PRIVATE));
        assert!(!fields.to_string().contains("discarded_gate"));
    }
}
