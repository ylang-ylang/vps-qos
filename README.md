# VPS Bandwidth Observer

A standalone Rust implementation of a passive VPS bandwidth observer and QoS
ceiling selector. It reads cumulative Linux `/proc` counters, converts interface
byte deltas into RX/TX bit rates, retains the maximum observation in a bounded
time window, and fuses independent congestion-factor reports with a scalar
Kalman filter. It never generates traffic or changes HAProxy itself.

## Why a BBR-style windowed maximum

Delivery-rate observations are structurally one-sided: application limitation,
cross traffic, and an unfilled pipe make a sample lower than the path ceiling.
A conventional Kalman filter assumes approximately symmetric, zero-mean
measurement noise and averages these low samples into a systematically low
estimate. The BBR family instead uses a windowed maximum: one observation that
fills the pipe raises the lower-bound estimate immediately, while expiration
allows sustained capacity reductions to be learned. This implementation uses
three equal rotating sub-windows, retaining one extremum per sub-window, for
constant memory and constant work per sample. It also exposes the symmetric
windowed-minimum form for future RTT propagation-baseline observations.

There is no seeded watermark, asymmetric confidence rule, signal weighting, or
warm-up threshold. A low first sample cannot suppress a later high sample.
Raw retransmission, timeout, and zero-window counters are collected. Their
counter deltas are exposed as uniform boolean factor reports carrying the
current direction's rate; no factor has a hard-coded priority or initial
weight.

Factors are modular. Each signal is an independent `Factor` implementation in
`src/factors.rs`; `all_factors()` constructs the production registry, and
`observe_all()` evaluates any supplied registry without policy or priority.
To add a factor, implement the trait and register one instance in
`all_factors()`; the collector, Kalman fusion, and main loop require no changes.

## Kalman congestion fusion and selected ceiling

The maximum filter and Kalman layer solve different problems. The maximum
filter preserves the best recent passive rate observation. The scalar Kalman
state represents congestion degree from 0 to 1. A triggered factor's recorded
speed is normalized into a congestion observation as
`1 - speed / nominal_ceiling`, clamped to `[0, 1]`. Independently triggered
factors are applied as sequential same-timestamp updates, which is equivalent
to multiplying their Gaussian likelihoods. A false factor is no observation,
not evidence of zero congestion.

Every factor begins with the same configured measurement variance. Its variance
then adapts from squared innovation: reports inconsistent with the shared state
become noisier and receive less gain, while consistently useful reports regain
influence. The configured process model also returns congestion toward zero as
time passes without reinforcing evidence. If congestion exceeds
`congestion_threshold`, the selected ceiling
is the recent window maximum (bounded by the nominal ceiling); otherwise it is
the configured nominal ceiling. The JSON output includes both window maxima,
both congestion states, and both selected ceilings.

## Configuration

Run with `vps-bandwidth-observer [CONFIG_PATH]`; the default path is
`config/default.json`. The file is grouped by operator responsibility:

- `required`: deployment-specific input. `nominal_ceiling_bps` has no serde
  fallback and configuration loading fails if this section is omitted. Change
  the checked-in example to match the advertised ceiling of the target VPS.
- `windowed_max_filter`: optional observer, procfs, state, and sampling
  overrides.
- `congestion_detection`: optional policy threshold and Kalman overrides.
- `factors`: per-factor enable switches and trigger parameters. Unknown factor
  names or fields are rejected so spelling errors cannot silently disable a signal.
- `web`: built-in dashboard enable switch, HTTP port, and retained history size.
- `_comment`: operator-facing documentation; ignored by runtime behavior.

Every operational parameter is externalized:

| Field | Example/default | Meaning/source |
|---|---:|---|
| `required.nominal_ceiling_bps` | 200000000 | **Required** operator-provided VPS ceiling used for normalization and uncongested output. |
| `windowed_max_filter.window_seconds` | 60 | Total retention horizon. It is time-based because the process observes an interface rather than per-flow RTT rounds. |
| `windowed_max_filter.sample_interval_seconds` | 2 | `/proc` sampling cadence; preserves the requested deployment cadence. |
| `windowed_max_filter.state_path` | `state/window.json` | Atomic JSON snapshot destination. |
| `windowed_max_filter.channel_id` | `default` | Output/state identity reserved for future channel routing. |
| `windowed_max_filter.interface` | `eth0` | Interface row selected in `/proc/net/dev`. |
| `windowed_max_filter.proc_root` | `/proc` | Procfs root; injectable for tests and container deployments. |
| `congestion_detection.congestion_threshold` | 0.7 | Congestion state above which the recent maximum is selected. |
| `congestion_detection.kalman.process_noise_per_second` | 0.005 | Random-walk covariance growth rate. |
| `congestion_detection.kalman.mean_reversion_per_second` | 0.01 | Per-second recovery toward no congestion when no evidence reinforces the state. |
| `congestion_detection.kalman.initial_measurement_noise` | 0.1 | Equal starting measurement variance for every factor. |
| `congestion_detection.kalman.measurement_noise_learning_rate` | 0.1 | Squared-innovation variance adaptation rate. |
| `congestion_detection.kalman.minimum_measurement_noise` | 0.001 | Lower variance bound. |
| `congestion_detection.kalman.maximum_measurement_noise` | 1 | Upper variance bound. |
| `congestion_detection.kalman.initial_state` | 0 | Initial congestion state. |
| `congestion_detection.kalman.initial_covariance` | 1 | Initial state uncertainty. |
| `web.enabled` | true | Starts the built-in HTTP dashboard when enabled. |
| `web.port` | 8080 | Dashboard listen port on all interfaces. |
| `web.history_ticks` | 300 | In-memory ring-buffer capacity; at the default two-second cadence this retains ten minutes. |

### Factor data sources and deployment requirements

| Factor | Default | Data source | Requirement |
|---|---|---|---|
| `retransmission` | enabled | `/proc/net/snmp` `RetransSegs` delta | Host procfs mount |
| `timeout` | enabled | `/proc/net/netstat` `TCPTimeouts` delta | Host procfs mount |
| `zero_window` | enabled | `/proc/net/netstat` zero-window deltas | Host procfs mount |
| `rate_stability` | disabled | configured-length RX/TX rate history | Host procfs mount |
| `rate_slope_zero` | disabled | normalized linear-regression slope of rate history | Host procfs mount |
| `rtt_inflation` | disabled | `fping -C 3` samples and rolling baseline | `fping`, network access, normally `NET_RAW` |
| `rtt_jitter` | disabled | coefficient of variation of the same `fping` samples | `fping`, network access, normally `NET_RAW` |
| `cwnd_shrink` | disabled | average `cwnd:` parsed from `ss -tiH` | `iproute2` (`ss`), host network namespace |
| `conn_up_throughput_flat` | disabled | system-wide `/proc/net/sockstat` `TCP: inuse` trend plus the configured `rate_slope_zero` history window/slope threshold | Host procfs mount |

The image includes `fping` and `iproute2`. Optional collectors are best-effort:
missing tools or unparseable output produce no factor observation rather than
terminating the observer. A missing or malformed `sockstat` connection count is
recorded as zero; the connection factor treats zero as unavailable and resets
its history. Enable or disable a factor
with `factors.<name>.enabled`; its thresholds live beside that switch in
`config/default.json`. RTT jitter uses the target list in
`factors.rtt_inflation.fping_targets` so only one fping invocation is needed.

The three sub-windows are an algorithm invariant and are deliberately not
configurable. A test requires `config/default.json` to equal
`RuntimeConfig::default()` exactly, preventing hidden/default drift.

Each JSON output line contains observed RX (`observed_down_bps`) and TX
(`observed_up_bps`) rates, their current window maxima, direction-specific
congestion states and selected ceilings, channel/timestamp, and raw TCP
counters. The first collection establishes a counter baseline and emits
nothing.

## Built-in web dashboard

With `web.enabled` set to `true`, open `http://<vps-address>:8080/` (or the
configured `web.port`). The single-page dashboard displays current up/down
estimates and congestion states, a retained download/congestion chart, and
recent factor triggers. It polls `GET /api/metrics` every two seconds. Metrics
history is memory-only and bounded by `web.history_ticks`; restarting the
process clears it. Chart.js is loaded from jsDelivr, so the charts require the
browser to reach that CDN, while the JSON API and current values are served
entirely by vps-qos.

When using the provided host-network Docker Compose deployment, no `ports:`
mapping is required; allow the configured TCP port through the VPS firewall if
remote dashboard access is desired.

## Build and verify

```sh
cargo test
cargo clippy --all-targets -- -D warnings
docker build -t vps-bandwidth-observer .
docker run --rm --network host --cap-add NET_RAW \
  -v /proc:/host-proc:ro \
  -v "$PWD/config:/app/config:ro" \
  -v "$PWD/state:/app/state" \
  vps-bandwidth-observer
# Dashboard: http://localhost:8080/
# JSON API: curl http://localhost:8080/api/metrics
```

For a container, set `proc_root` to `/host-proc` in the mounted configuration.
The observer must see the host interface counters; host networking alone does
not replace the procfs mount.

## Docker Compose deployment

A `docker-compose.yaml` is provided with all required permissions and mounts:

```bash
docker compose up -d
```

This includes:
- `NET_RAW` and `NET_ADMIN` capabilities (for `fping` and `ss -ti`)
- Host network and PID namespace (to see real interfaces and processes)
- `/proc` mount (to read host network/TCP counters)
- `/var/lib/qos` state persistence
- `/etc/vps-qos` config override directory

Before starting, create the state and config directories and edit the config:

```bash
sudo mkdir -p /var/lib/qos /etc/vps-qos
sudo cp config/default.json /etc/vps-qos/default.json
# Edit /etc/vps-qos/default.json: set required.nominal_ceiling_bps and enable desired factors
```

Then start with:

```bash
docker compose up -d
docker compose logs -f
```
