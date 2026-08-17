use serde::Serialize;
use std::error::Error;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use vps_bandwidth_observer::collector::{ProcCollector, RawCounters};
use vps_bandwidth_observer::config::RuntimeConfig;
use vps_bandwidth_observer::estimator::{Extremum, WindowedExtremum};
use vps_bandwidth_observer::factors;
use vps_bandwidth_observer::kalman::CongestionKalman;
use vps_bandwidth_observer::policy::select_ceiling_bps;
use vps_bandwidth_observer::state::ObserverState;

#[derive(Serialize)]
struct Output<'a> {
    channel_id: &'a str,
    timestamp: f64,
    observed_down_bps: f64,
    observed_up_bps: f64,
    window_max_down_bps: f64,
    window_max_up_bps: f64,
    congestion_down: f64,
    congestion_up: f64,
    estimate_down_bps: f64,
    estimate_up_bps: f64,
    counters: &'a RawCounters,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config/default.json".to_owned());
    let config = RuntimeConfig::load(config_path)?;
    let window = &config.windowed_max_filter;
    let congestion = &config.congestion_detection;
    let nominal_ceiling_bps = config.required.nominal_ceiling_bps;
    let mut state = restore_or_create(&config)?;
    let mut collector = ProcCollector::new(&window.proc_root, &window.interface);
    let mut down_congestion = CongestionKalman::new(congestion.kalman.clone())?;
    let mut up_congestion = CongestionKalman::new(congestion.kalman.clone())?;
    let factor_registry = factors::all_factors();
    let interval = Duration::from_secs_f64(window.sample_interval_seconds);

    loop {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
        let previous_counters = collector.previous_counters().cloned();
        if let Some(sample) = collector.sample(timestamp)? {
            let window_max_down_bps = state.down_max.feed(sample.down_bps, sample.timestamp)?;
            let window_max_up_bps = state.up_max.feed(sample.up_bps, sample.timestamp)?;
            let reports_down = previous_counters
                .as_ref()
                .map(|previous| {
                    factors::observe_all(
                        &factor_registry,
                        previous,
                        &sample.counters,
                        sample.down_bps,
                    )
                })
                .unwrap_or_default();
            let reports_up = previous_counters
                .as_ref()
                .map(|previous| {
                    factors::observe_all(
                        &factor_registry,
                        previous,
                        &sample.counters,
                        sample.up_bps,
                    )
                })
                .unwrap_or_default();
            let congestion_down =
                down_congestion.process(sample.timestamp, nominal_ceiling_bps, &reports_down)?;
            let congestion_up =
                up_congestion.process(sample.timestamp, nominal_ceiling_bps, &reports_up)?;
            let estimate_down_bps = select_ceiling_bps(
                congestion_down,
                congestion.congestion_threshold,
                nominal_ceiling_bps,
                window_max_down_bps,
            );
            let estimate_up_bps = select_ceiling_bps(
                congestion_up,
                congestion.congestion_threshold,
                nominal_ceiling_bps,
                window_max_up_bps,
            );
            let output = Output {
                channel_id: &state.channel_id,
                timestamp: sample.timestamp,
                observed_down_bps: sample.down_bps,
                observed_up_bps: sample.up_bps,
                window_max_down_bps,
                window_max_up_bps,
                congestion_down,
                congestion_up,
                estimate_down_bps,
                estimate_up_bps,
                counters: &sample.counters,
            };
            println!("{}", serde_json::to_string(&output)?);
            state.save_atomic(&window.state_path)?;
        }
        thread::sleep(interval);
    }
}

fn restore_or_create(config: &RuntimeConfig) -> Result<ObserverState, Box<dyn Error>> {
    let window = &config.windowed_max_filter;
    if let Some(state) = ObserverState::load(&window.state_path)?
        && state.channel_id == window.channel_id
        && state.down_max.kind() == Extremum::Max
        && state.up_max.kind() == Extremum::Max
        && state.down_max.window_seconds() == window.window_seconds
        && state.up_max.window_seconds() == window.window_seconds
    {
        return Ok(state);
    }
    Ok(ObserverState {
        channel_id: window.channel_id.clone(),
        down_max: WindowedExtremum::max(window.window_seconds)?,
        up_max: WindowedExtremum::max(window.window_seconds)?,
    })
}
