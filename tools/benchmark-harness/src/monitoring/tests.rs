use super::*;

#[test]
fn test_adaptive_sampling_interval_small_file() {
    let interval = adaptive_sampling_interval_ms(50_000);
    assert_eq!(interval, 1, "Small file (50KB) should use 1ms interval");
}

#[test]
fn test_adaptive_sampling_interval_boundary_100kb() {
    let interval = adaptive_sampling_interval_ms(100_000);
    assert_eq!(interval, 5, "Exactly 100KB boundary should use 5ms interval");
}

#[test]
fn test_adaptive_sampling_interval_medium_file() {
    let interval = adaptive_sampling_interval_ms(1_000_000);
    assert_eq!(interval, 5, "Medium file (1MB) should use 5ms interval");
}

#[test]
fn test_adaptive_sampling_interval_boundary_10mb() {
    let interval = adaptive_sampling_interval_ms(10_000_000);
    assert_eq!(interval, 10, "Exactly 10MB boundary should use 10ms interval");
}

#[test]
fn test_adaptive_sampling_interval_large_file() {
    let interval = adaptive_sampling_interval_ms(100_000_000);
    assert_eq!(interval, 10, "Large file (100MB) should use 10ms interval");
}

#[test]
fn test_adaptive_sampling_interval_zero_bytes() {
    let interval = adaptive_sampling_interval_ms(0);
    assert_eq!(interval, 1, "Zero byte file should use 1ms interval");
}

#[test]
fn test_adaptive_sampling_interval_max_u64() {
    let interval = adaptive_sampling_interval_ms(u64::MAX);
    assert_eq!(interval, 10, "u64::MAX should use 10ms interval");
}

#[test]
fn test_calculate_percentile() {
    let values = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    assert_eq!(ResourceMonitor::calculate_percentile(values.clone(), 0.0), 1);
    assert_eq!(ResourceMonitor::calculate_percentile(values.clone(), 0.5), 5);
    assert_eq!(ResourceMonitor::calculate_percentile(values.clone(), 0.95), 9);
    assert_eq!(ResourceMonitor::calculate_percentile(values, 1.0), 10);
}

#[test]
fn test_calculate_percentile_single_value() {
    let values = vec![42];
    assert_eq!(ResourceMonitor::calculate_percentile(values, 0.5), 42);
}

#[test]
fn test_calculate_percentile_empty() {
    let values = vec![];
    assert_eq!(ResourceMonitor::calculate_percentile(values, 0.5), 0);
}

#[test]
fn integrate_cpu_core_seconds_returns_zero_for_fewer_than_two_samples() {
    assert_eq!(integrate_cpu_core_seconds(&[], 4.0), 0.0);

    let single = [ResourceSample {
        memory_bytes: 0,
        vm_size_bytes: 0,
        page_faults: 0,
        cpu_percent: 50.0,
        timestamp_ms: 0,
    }];
    assert_eq!(integrate_cpu_core_seconds(&single, 4.0), 0.0);
}

#[test]
fn integrate_cpu_core_seconds_trapezoidal_two_samples() {
    let samples = [
        ResourceSample {
            memory_bytes: 0,
            vm_size_bytes: 0,
            page_faults: 0,
            cpu_percent: 25.0,
            timestamp_ms: 0,
        },
        ResourceSample {
            memory_bytes: 0,
            vm_size_bytes: 0,
            page_faults: 0,
            cpu_percent: 50.0,
            timestamp_ms: 1_000,
        },
    ];

    let core_seconds = integrate_cpu_core_seconds(&samples, 2.0);

    assert!(
        (core_seconds - 0.75).abs() < 1e-9,
        "expected 0.75 core-seconds, got {core_seconds}"
    );
}

#[test]
fn integrate_cpu_core_seconds_sums_across_multiple_windows() {
    let samples = [
        ResourceSample {
            memory_bytes: 0,
            vm_size_bytes: 0,
            page_faults: 0,
            cpu_percent: 25.0,
            timestamp_ms: 0,
        },
        ResourceSample {
            memory_bytes: 0,
            vm_size_bytes: 0,
            page_faults: 0,
            cpu_percent: 25.0,
            timestamp_ms: 500,
        },
        ResourceSample {
            memory_bytes: 0,
            vm_size_bytes: 0,
            page_faults: 0,
            cpu_percent: 25.0,
            timestamp_ms: 1_000,
        },
    ];

    let core_seconds = integrate_cpu_core_seconds(&samples, 4.0);

    assert!(
        (core_seconds - 1.0).abs() < 1e-9,
        "expected 1.0 core-second, got {core_seconds}"
    );
}

#[tokio::test]
async fn test_resource_monitor_basic() {
    let monitor = ResourceMonitor::new();

    monitor.start(Duration::from_millis(25)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let samples = monitor.stop().await;

    assert!(!samples.is_empty(), "Should have collected samples");
    assert!(samples.len() >= 2, "Should have at least 2 samples");
}

#[tokio::test]
async fn start_waits_for_initial_sample() {
    let monitor = ResourceMonitor::new();

    monitor.start(Duration::from_secs(1)).await;
    let baseline = monitor.baseline_memory().await;
    let samples = tokio::time::timeout(Duration::from_millis(250), monitor.stop())
        .await
        .expect("stop must wake a sampler with a long interval promptly");

    assert!(baseline > 0, "start must capture the baseline before returning");
    assert_eq!(samples.len(), 1, "the initial sample must not depend on the interval");
    assert_eq!(samples[0].memory_bytes, baseline);
}

#[tokio::test]
async fn prepare_captures_baseline_without_recording_a_sample() {
    let monitor = ResourceMonitor::new();

    monitor.prepare().await;
    let baseline = monitor.baseline_memory().await;
    let samples = monitor.stop().await;

    assert!(baseline > 0, "prepare must capture baseline RSS");
    assert!(samples.is_empty(), "prepare must not record baseline RSS as a sample");
}

#[tokio::test]
async fn cancelled_start_does_not_poison_monitor_lifecycle() {
    let monitor = Arc::new(ResourceMonitor::new());
    let sampler_guard = monitor.sampler.lock().await;
    let starting_monitor = Arc::clone(&monitor);
    let start_task = tokio::spawn(async move {
        starting_monitor.start(Duration::from_millis(1)).await;
    });
    tokio::task::yield_now().await;
    assert!(!start_task.is_finished(), "start must be waiting for sampler ownership");

    start_task.abort();
    assert!(start_task.await.unwrap_err().is_cancelled());
    drop(sampler_guard);

    tokio::time::timeout(Duration::from_secs(1), monitor.start(Duration::from_millis(1)))
        .await
        .expect("a cancelled start must not leave the monitor marked as running");
    tokio::time::sleep(Duration::from_millis(5)).await;
    let samples = tokio::time::timeout(Duration::from_secs(1), monitor.stop())
        .await
        .expect("monitor must remain stoppable after restarting");
    assert!(!samples.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn sampling_does_not_starve_current_thread_runtime() {
    const HEARTBEAT_COUNT: usize = 20;
    const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(2);
    const HEARTBEAT_DEADLINE: Duration = Duration::from_millis(250);

    let monitor = ResourceMonitor::new();
    monitor.start(Duration::from_millis(1)).await;

    let heartbeat = tokio::time::timeout(HEARTBEAT_DEADLINE, async {
        for _ in 0..HEARTBEAT_COUNT {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
        }
    })
    .await;
    let samples = monitor.stop().await;

    assert!(heartbeat.is_ok(), "resource sampling starved the Tokio runtime");
    assert!(samples.len() >= 2, "background sampling did not remain active");
}

#[tokio::test]
async fn test_resource_stats_calculation() {
    let samples = vec![
        ResourceSample {
            memory_bytes: 100,
            vm_size_bytes: 500,
            page_faults: 10,
            cpu_percent: 10.0,
            timestamp_ms: 0,
        },
        ResourceSample {
            memory_bytes: 200,
            vm_size_bytes: 600,
            page_faults: 20,
            cpu_percent: 20.0,
            timestamp_ms: 10,
        },
        ResourceSample {
            memory_bytes: 150,
            vm_size_bytes: 550,
            page_faults: 25,
            cpu_percent: 15.0,
            timestamp_ms: 20,
        },
    ];

    let snapshots = vec![
        MemorySnapshot::new(
            Duration::from_millis(0),
            100,
            500,
            10,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
        MemorySnapshot::new(
            Duration::from_millis(10),
            200,
            600,
            20,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
        MemorySnapshot::new(
            Duration::from_millis(20),
            150,
            550,
            25,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
    ];

    let stats = ResourceMonitor::calculate_stats(&samples, &snapshots, 100);

    assert_eq!(stats.baseline_memory_bytes, 100);
    assert_eq!(stats.peak_memory_bytes, 200);
    assert_eq!(stats.peak_memory_delta_bytes, 100);
    assert_eq!(stats.peak_vm_bytes, 600);
    assert_eq!(stats.total_page_faults, 25);
    assert_eq!(stats.p50_memory_bytes, 150);
    assert!((stats.avg_cpu_percent - 15.0).abs() < 0.1);
    assert_eq!(stats.sample_count, 3);
    assert!(stats.memory_growth_rate_mb_s >= 0.0);
    assert_eq!(stats.snapshots.len(), 3);
    // `calculate_stats` must wire `cpu_seconds` through the same integration function,
    // called with the host's actual logical core count (not asserted as a machine-dependent
    // literal, since `num_cpus::get()` varies across CI runners). ~keep
    let expected_core_seconds = integrate_cpu_core_seconds(&samples, num_cpus::get() as f64);
    assert!(
        (stats.cpu_seconds - expected_core_seconds).abs() < 1e-9,
        "expected cpu_seconds {expected_core_seconds}, got {}",
        stats.cpu_seconds
    );
}

#[tokio::test]
async fn test_resource_stats_empty() {
    let stats = ResourceMonitor::calculate_stats(&[], &[], 42);
    assert_eq!(stats.baseline_memory_bytes, 42);
    assert_eq!(stats.peak_memory_bytes, 0);
    assert_eq!(stats.sample_count, 0);
    assert_eq!(stats.cpu_seconds, 0.0);
}

#[tokio::test]
async fn test_leak_detection() {
    let snapshots = vec![
        MemorySnapshot::new(
            Duration::from_millis(0),
            1000,
            5000,
            0,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
        MemorySnapshot::new(
            Duration::from_millis(10),
            2000,
            6000,
            0,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
        MemorySnapshot::new(
            Duration::from_millis(20),
            1200,
            5500,
            0,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
    ];

    let samples = vec![ResourceSample {
        memory_bytes: 1200,
        vm_size_bytes: 5500,
        page_faults: 0,
        cpu_percent: 0.0,
        timestamp_ms: 20,
    }];
    let stats = ResourceMonitor::calculate_stats(&samples, &snapshots, 0);
    assert!(
        stats.leak_detected,
        "Should detect leak with >5% growth and >20% retention"
    );
}

#[tokio::test]
async fn test_no_leak_detection_temporary_spike() {
    let snapshots = vec![
        MemorySnapshot::new(
            Duration::from_millis(0),
            1000,
            5000,
            0,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
        MemorySnapshot::new(
            Duration::from_millis(10),
            5000,
            9000,
            0,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
        MemorySnapshot::new(
            Duration::from_millis(20),
            1001,
            5001,
            0,
            #[cfg(feature = "memory-profiling")]
            None,
        ),
    ];

    let samples = vec![ResourceSample {
        memory_bytes: 1001,
        vm_size_bytes: 5001,
        page_faults: 0,
        cpu_percent: 0.0,
        timestamp_ms: 20,
    }];
    let stats = ResourceMonitor::calculate_stats(&samples, &snapshots, 0);
    assert!(!stats.leak_detected, "Should not detect leak when memory is released");
}

#[tokio::test]
async fn test_snapshot_collection() {
    let monitor = ResourceMonitor::new();

    monitor.start(Duration::from_millis(10)).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let snapshots = monitor.get_snapshots().await;
    assert!(
        !snapshots.is_empty(),
        "Should have collected snapshots during monitoring"
    );

    let peak = monitor.peak_snapshot().await;
    assert!(peak.is_some(), "Should find peak snapshot");

    let trajectory = monitor.growth_trajectory().await;
    assert_eq!(
        trajectory.len(),
        snapshots.len(),
        "Trajectory should match snapshot count"
    );

    monitor.stop().await;
}
