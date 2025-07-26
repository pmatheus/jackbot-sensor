use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from("../jackbot-backend/proto");
    
    // Compile Protocol Buffer files
    let proto_files = [
        proto_root.join("market_data.proto"),
        proto_root.join("trading.proto"),
        proto_root.join("execution.proto"),
        proto_root.join("sensor_health.proto"),
    ];

    let includes = [&proto_root];

    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(&proto_files, &includes)?;

    // Re-run if proto files change
    for file in &proto_files {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    Ok(())
}