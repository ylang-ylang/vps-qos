use axum::{Json, Router, extract::State, http::header, response::Html, routing::get};
use serde::Serialize;
use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetricsPoint {
    pub timestamp: f64,
    pub estimate_down_bps: f64,
    pub estimate_up_bps: f64,
    pub congestion_down: f64,
    pub congestion_up: f64,
    pub window_max_down_bps: f64,
    pub window_max_up_bps: f64,
    pub triggered_factors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetricsResponse {
    pub current: Option<MetricsPoint>,
    pub history: Vec<MetricsPoint>,
}

#[derive(Debug)]
struct History {
    capacity: usize,
    points: VecDeque<MetricsPoint>,
}

#[derive(Clone, Debug)]
pub struct MetricsStore {
    inner: Arc<Mutex<History>>,
}

impl MetricsStore {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "metrics history capacity must be positive");
        Self {
            inner: Arc::new(Mutex::new(History {
                capacity,
                points: VecDeque::with_capacity(capacity),
            })),
        }
    }

    pub fn push(&self, point: MetricsPoint) {
        let mut history = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if history.points.len() == history.capacity {
            history.points.pop_front();
        }
        history.points.push_back(point);
    }

    pub fn snapshot(&self) -> MetricsResponse {
        let history = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        MetricsResponse {
            current: history.points.back().cloned(),
            history: history.points.iter().cloned().collect(),
        }
    }
}

pub fn router(store: MetricsStore) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/metrics", get(metrics))
        .with_state(store)
}

pub fn spawn(port: u16, store: MetricsStore) -> io::Result<thread::JoinHandle<()>> {
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = std::net::TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;

    thread::Builder::new()
        .name("vps-qos-web".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("cannot start web runtime");
            runtime.block_on(async move {
                let listener =
                    tokio::net::TcpListener::from_std(listener).expect("cannot adopt web listener");
                if let Err(error) = axum::serve(listener, router(store)).await {
                    eprintln!("web server stopped: {error}");
                }
            });
        })
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn metrics(
    State(store): State<MetricsStore>,
) -> (
    [(header::HeaderName, &'static str); 1],
    Json<MetricsResponse>,
) {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(store.snapshot()),
    )
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>VPS QoS bandwidth observer</title>
  <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
  <style>
    :root { color-scheme: dark; font-family: system-ui, sans-serif; }
    body { margin: 0; padding: 24px; background: #111827; color: #f9fafb; }
    h1 { margin-top: 0; }
    .cards { display: grid; grid-template-columns: repeat(auto-fit,minmax(210px,1fr)); gap: 12px; }
    .card, .panel { background: #1f2937; border-radius: 10px; padding: 16px; }
    .value { font-size: 2rem; font-weight: 700; margin-top: 6px; }
    .panels { display: grid; gap: 16px; margin-top: 16px; }
    .chart-wrap { height: 360px; }
    table { width: 100%; border-collapse: collapse; }
    th, td { text-align: left; padding: 8px; border-bottom: 1px solid #374151; }
    .muted { color: #9ca3af; }
  </style>
</head>
<body>
  <h1>VPS QoS bandwidth observer</h1>
  <div class="cards">
    <div class="card">Estimated download<div id="down" class="value">--</div></div>
    <div class="card">Estimated upload<div id="up" class="value">--</div></div>
    <div class="card">Download congestion<div id="congDown" class="value">--</div></div>
    <div class="card">Upload congestion<div id="congUp" class="value">--</div></div>
  </div>
  <div class="panels">
    <div class="panel chart-wrap"><canvas id="historyChart"></canvas></div>
    <div class="panel">
      <h2>Recent factor triggers</h2>
      <table><thead><tr><th>Time</th><th>Factor</th><th>Estimated download</th></tr></thead><tbody id="factorRows"></tbody></table>
    </div>
  </div>
<script>
const fmtRate = value => value == null ? '--' : `${(value / 1e6).toFixed(2)} Mbps`;
const fmtCongestion = value => value == null ? '--' : `${(value * 100).toFixed(1)}%`;
const chart = new Chart(document.getElementById('historyChart'), {
  type: 'line',
  data: { datasets: [
    { label: 'Estimated download (Mbps)', data: [], borderColor: '#38bdf8', yAxisID: 'rate', pointRadius: 0 },
    { label: 'Download congestion', data: [], borderColor: '#fb7185', yAxisID: 'congestion', pointRadius: 0 }
  ]},
  options: { responsive: true, maintainAspectRatio: false, parsing: false,
    scales: { x: { type: 'category' }, rate: { position: 'left', beginAtZero: true }, congestion: { position: 'right', min: 0, max: 1, grid: { drawOnChartArea: false } } }
  }
});
async function refresh() {
  try {
    const response = await fetch('/api/metrics', { cache: 'no-store' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const metrics = await response.json();
    const current = metrics.current;
    document.getElementById('down').textContent = fmtRate(current?.estimate_down_bps);
    document.getElementById('up').textContent = fmtRate(current?.estimate_up_bps);
    document.getElementById('congDown').textContent = fmtCongestion(current?.congestion_down);
    document.getElementById('congUp').textContent = fmtCongestion(current?.congestion_up);
    chart.data.labels = metrics.history.map(point => new Date(point.timestamp * 1000).toLocaleTimeString());
    chart.data.datasets[0].data = metrics.history.map(point => point.estimate_down_bps / 1e6);
    chart.data.datasets[1].data = metrics.history.map(point => point.congestion_down);
    chart.update('none');
    const triggers = metrics.history.flatMap(point => point.triggered_factors.map(name => ({ name, point }))).slice(-50).reverse();
    document.getElementById('factorRows').innerHTML = triggers.length ? triggers.map(({name, point}) =>
      `<tr><td>${new Date(point.timestamp * 1000).toLocaleString()}</td><td>${name}</td><td>${fmtRate(point.estimate_down_bps)}</td></tr>`).join('') :
      '<tr><td colspan="3" class="muted">No factor triggers in retained history</td></tr>';
  } catch (error) { console.error('metrics refresh failed', error); }
}
refresh();
setInterval(refresh, 2000);
</script>
</body>
</html>"#;
