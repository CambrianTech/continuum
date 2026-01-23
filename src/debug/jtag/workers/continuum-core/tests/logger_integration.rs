/// Integration test for logger client
///
/// Tests that continuum-core can connect to the existing logger worker
/// and send log messages via Unix socket.

use continuum_core::{init_logger, logger};

#[test]
fn test_logger_connection() {
    // Initialize logger with the standard socket path
    let socket_path = "/tmp/jtag-logger-worker.sock";

    match init_logger(socket_path) {
        Ok(_) => {
            println!("✅ Logger initialized successfully");
        }
        Err(e) => {
            panic!("❌ Failed to initialize logger: {e}");
        }
    }

    // Send test messages at different levels
    logger().debug("test", "logger_integration", "Debug message from continuum-core");
    logger().info("test", "logger_integration", "Info message from continuum-core");
    logger().warn("test", "logger_integration", "Warning message from continuum-core");
    logger().error("test", "logger_integration", "Error message from continuum-core");

    // Give logger time to write
    std::thread::sleep(std::time::Duration::from_millis(100));

    println!("✅ Sent 4 test log messages");
}

#[test]
fn test_logger_with_timing() {
    use continuum_core::logging::TimingGuard;

    let socket_path = "/tmp/jtag-logger-worker.sock";
    init_logger(socket_path).expect("Failed to init logger");

    // Test timing guard
    {
        let _timer = TimingGuard::new("test", "timing_test_operation");

        // Simulate work
        std::thread::sleep(std::time::Duration::from_micros(500));

        // Timer will log automatically on drop
    }

    // Give logger time to write
    std::thread::sleep(std::time::Duration::from_millis(100));

    println!("✅ Timing guard test completed");
}

#[test]
fn test_logger_performance() {
    let socket_path = "/tmp/jtag-logger-worker.sock";
    init_logger(socket_path).expect("Failed to init logger");

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

    println!("✅ 1000 messages in {elapsed:?}");
    println!("   Average: {per_message}μs per message");

    // Should be fast (non-blocking)
    assert!(per_message < 100, "Logging is too slow: {per_message}μs per message");
}
