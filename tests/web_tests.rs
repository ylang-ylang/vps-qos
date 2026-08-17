use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use vps_bandwidth_observer::web::{self, MetricsPoint, MetricsStore};

fn point(timestamp: f64, factor: &str) -> MetricsPoint {
    MetricsPoint {
        timestamp,
        estimate_down_bps: 100_000_000.0 + timestamp,
        estimate_up_bps: 50_000_000.0,
        congestion_down: 0.4,
        congestion_up: 0.2,
        window_max_down_bps: 110_000_000.0,
        window_max_up_bps: 55_000_000.0,
        triggered_factors: vec![factor.to_owned()],
    }
}

#[test]
fn history_is_bounded_and_current_is_newest() {
    let store = MetricsStore::new(2);
    store.push(point(1.0, "first"));
    store.push(point(2.0, "second"));
    store.push(point(3.0, "third"));

    let snapshot = store.snapshot();
    assert_eq!(snapshot.history.len(), 2);
    assert_eq!(snapshot.history[0].timestamp, 2.0);
    assert_eq!(snapshot.current.unwrap().timestamp, 3.0);
}

#[tokio::test]
async fn metrics_endpoint_returns_expected_json_shape() {
    let store = MetricsStore::new(3);
    store.push(point(123.0, "retransmission"));
    let response = web::router(store)
        .oneshot(
            Request::builder()
                .uri("/api/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["current"]["timestamp"], 123.0);
    assert_eq!(json["current"]["triggered_factors"][0], "retransmission");
    assert_eq!(json["history"].as_array().unwrap().len(), 1);
    for field in [
        "estimate_down_bps",
        "estimate_up_bps",
        "congestion_down",
        "congestion_up",
        "window_max_down_bps",
        "window_max_up_bps",
    ] {
        assert!(json["current"].get(field).is_some(), "missing {field}");
    }
}

#[tokio::test]
async fn root_serves_chart_page() {
    let response = web::router(MetricsStore::new(1))
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("cdn.jsdelivr.net/npm/chart.js"));
    assert!(html.contains("/api/metrics"));
}
