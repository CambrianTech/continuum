fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Proto compilation is optional - skip if protoc not available
    // This allows building the library without gRPC initially

    // Create output directory if needed
    std::fs::create_dir_all("src/proto").ok();

    // List of proto files to compile
    let protos = [
        ("proto/streaming.proto", "streaming"),
        ("proto/voice.proto", "voice"),
    ];

    for (proto_path, name) in protos {
        if std::path::Path::new(proto_path).exists() {
            match tonic_build::configure()
                .build_server(true)
                .build_client(true)
                .out_dir("src/proto")
                .compile(&[proto_path], &["proto"])
            {
                Ok(_) => println!("cargo:warning={} proto compilation successful", name),
                Err(e) => {
                    println!("cargo:warning={} proto compilation skipped: {}", name, e);
                    println!("cargo:warning=Install protoc to enable gRPC service");
                }
            }
        }
    }

    Ok(())
}
