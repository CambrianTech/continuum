/// Integration test for logger client
///
/// Tests that continuum-core can connect to the existing logger worker
/// and send log messages via Unix socket.

use continuum_core::{init_logger, logger};
use std::sync::Once;

static LOGGER_INIT: Once = Once::new();

/// Initialize logger once for all tests (global singleton).
fn ensure_logger() {
    LOGGER_INIT.call_once(|| {
        let socket_path = "/tmp/jtag-logger-worker.sock";
        if let Err(e) = init_logger(socket_path) {
            eprintln!("Logger init failed (expected if logger worker not running): {e}");
        }
    });
}

#[test]
fn test_logger_connection() {
    ensure_logger();

    // Send test messages at different levels
    logger().debug("test", "logger_integration", "Debug message from continuum-core");
    logger().info("test", "logger_integration", "Info message from continuum-core");
    logger().warn("test", "logger_integration", "Warning message from continuum-core");
    logger().error("test", "logger_integration", "Error message from continuum-core");

    // Give logger time to write
    std::thread::sleep(std::time::Duration::from_millis(100));

    println!("Sent 4 test log messages");
}

#[test]
fn test_logger_with_timing() {
    use continuum_core::logging::TimingGuard;

    ensure_logger();

    // Test timing guard
    {
        let _timer = TimingGuard::new("test", "timing_test_operation");

        // Simulate work
        std::thread::sleep(std::time::Duration::from_micros(500));

        // Timer will log automatically on drop
    }

    // Give logger time to write
    std::thread::sleep(std::time::Duration::from_millis(100));

    println!("Timing guard test completed");
}

#[test]
fn test_logger_performance() {
    ensure_logger();

    // Measure time to send 1000 log messages
    let start = std::time::Instant::now();

    for i in 0..1000 {
        logger().info(
            "test",
            "perf_test",
            &format!("Performance test message {i}")
        );
    }

    let elapsed = start.elapsed();
    let per_message = elapsed.as_micros() / 1000;

    println!("1000 messages in {elapsed:?}");
    println!("   Average: {per_message}us per message");

    // Should be fast (non-blocking)
    assert!(per_message < 100, "Logging is too slow: {per_message}us per message");
}
