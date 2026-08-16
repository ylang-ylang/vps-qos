use vps_bandwidth_observer::estimator::WindowedExtremum;
use vps_bandwidth_observer::state::ObserverState;

#[test]
fn persistence_round_trip_preserves_estimate_without_jump() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("nested/state.json");
    let mut state = ObserverState {
        channel_id: "default".to_owned(),
        down_max: WindowedExtremum::max(60.0).unwrap(),
        up_max: WindowedExtremum::max(60.0).unwrap(),
    };
    state.down_max.feed(188_000_000.0, 10.0).unwrap();
    state.up_max.feed(50_000_000.0, 10.0).unwrap();
    state.save_atomic(&path).unwrap();
    let restored = ObserverState::load(&path).unwrap().unwrap();
    assert_eq!(restored, state);
    assert_eq!(restored.down_max.estimate(), Some(188_000_000.0));
    assert_eq!(restored.up_max.estimate(), Some(50_000_000.0));
}
