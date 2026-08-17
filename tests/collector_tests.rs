use std::fs;
use vps_bandwidth_observer::collector::{
    ProcCollector, parse_average_cwnd, parse_fping_output, parse_haproxy_conn_cur,
};

fn write_proc(root: &std::path::Path, rx: u64, tx: u64, retrans: u64) {
    fs::create_dir_all(root.join("net")).unwrap();
    fs::write(
        root.join("net/dev"),
        format!(
            "Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n eth0: {rx} 1 0 0 0 0 0 0 {tx} 2 0 0 0 0 0 0\n"
        ),
    )
    .unwrap();
    fs::write(
        root.join("net/snmp"),
        format!("Tcp: ActiveOpens RetransSegs\nTcp: -1 {retrans}\n"),
    )
    .unwrap();
    fs::write(
        root.join("net/netstat"),
        "TcpExt: TCPTimeouts TCPToZeroWindowAdv TCPFromZeroWindowAdv\nTcpExt: 7 8 9\n",
    )
    .unwrap();
}

#[test]
fn reads_synthetic_proc_counters() {
    let temp = tempfile::tempdir().unwrap();
    write_proc(temp.path(), 1_000, 2_000, 6);
    let collector = ProcCollector::new(temp.path(), "eth0");
    let counters = collector.read_counters().unwrap();
    assert_eq!(counters.rx_bytes, 1_000);
    assert_eq!(counters.tx_bytes, 2_000);
    assert_eq!(counters.retransmitted_segments, 6);
    assert_eq!(counters.tcp_timeouts, 7);
    assert_eq!(counters.to_zero_window_advertisements, 8);
    assert_eq!(counters.from_zero_window_advertisements, 9);
}

#[test]
fn computes_bits_per_second_from_counter_deltas() {
    let temp = tempfile::tempdir().unwrap();
    write_proc(temp.path(), 1_000, 2_000, 6);
    let mut collector = ProcCollector::new(temp.path(), "eth0");
    assert!(collector.sample(10.0).unwrap().is_none());
    write_proc(temp.path(), 2_000, 2_500, 7);
    let sample = collector.sample(12.0).unwrap().unwrap();
    assert_eq!(sample.down_bps, 4_000.0);
    assert_eq!(sample.up_bps, 2_000.0);
    assert_eq!(sample.down_history_bps, vec![4_000.0]);
}

#[test]
fn parses_best_effort_auxiliary_tool_outputs() {
    assert_eq!(
        parse_fping_output("8.8.8.8 : 10.1 11.2 -"),
        vec![10.1, 11.2]
    );
    assert_eq!(
        parse_average_cwnd("cubic wscale:7,7 cwnd:10\n bbr cwnd:30"),
        Some(20.0)
    );
    assert_eq!(
        parse_haproxy_conn_cur(
            "# pxname,svname,scur,status\nfront,FRONTEND,2,OPEN\nback,srv,3,UP\n"
        ),
        Some(2)
    );
}
