use vps_bandwidth_observer::estimator::WindowedExtremum;

#[test]
fn max_tracks_increasing_and_decreasing_samples() {
    let mut filter = WindowedExtremum::max(60.0).unwrap();
    assert_eq!(filter.feed(10.0, 0.0).unwrap(), 10.0);
    assert_eq!(filter.feed(20.0, 2.0).unwrap(), 20.0);
    assert_eq!(filter.feed(15.0, 4.0).unwrap(), 20.0);
    assert_eq!(filter.feed(30.0, 22.0).unwrap(), 30.0);
    assert_eq!(filter.feed(5.0, 24.0).unwrap(), 30.0);
}

#[test]
fn oldest_subwindow_expiry_promotes_next_maximum() {
    let mut filter = WindowedExtremum::max(60.0).unwrap();
    filter.feed(100.0, 0.0).unwrap();
    filter.feed(80.0, 21.0).unwrap();
    filter.feed(70.0, 41.0).unwrap();
    assert_eq!(filter.feed(60.0, 60.0).unwrap(), 80.0);
    assert_eq!(filter.feed(50.0, 81.0).unwrap(), 70.0);
}

#[test]
fn cold_start_low_sample_never_blocks_later_high_sample() {
    let mut filter = WindowedExtremum::max(60.0).unwrap();
    assert_eq!(filter.feed(113_000.0, 0.0).unwrap(), 113_000.0);
    assert_eq!(filter.feed(188_000_000.0, 2.0).unwrap(), 188_000_000.0);
    assert_eq!(filter.feed(1_000.0, 4.0).unwrap(), 188_000_000.0);
}

#[test]
fn min_filter_is_symmetric() {
    let mut filter = WindowedExtremum::min(60.0).unwrap();
    assert_eq!(filter.feed(50.0, 0.0).unwrap(), 50.0);
    assert_eq!(filter.feed(10.0, 2.0).unwrap(), 10.0);
    assert_eq!(filter.feed(20.0, 4.0).unwrap(), 10.0);
}

#[test]
fn advance_expires_all_old_slots() {
    let mut filter = WindowedExtremum::max(60.0).unwrap();
    filter.feed(50.0, 0.0).unwrap();
    assert_eq!(filter.advance(60.0).unwrap(), None);
}
