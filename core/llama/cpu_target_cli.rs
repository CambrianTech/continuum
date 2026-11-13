//! Publisher adapter for the CPU policy also consumed by the core build script.
mod cpu_target;

fn main() {
    // Fail closed if accidentally invoked outside the explicit publisher mode.
    if std::env::var(cpu_target::PORTABLE_ENV).as_deref() != Ok("1") {
        std::process::exit(2);
    }
    for (name, value) in cpu_target::definitions("x86_64", true) {
        println!("-D{name}={value}");
    }
}
