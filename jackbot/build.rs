fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only compile proto if the file exists
    if std::path::Path::new("../protos/strategy.proto").exists() {
        tonic_build::configure()
            .build_server(false)
            .build_client(true)
            .compile(
                &["../protos/strategy.proto"],
                &["../protos"],
            )?;
    }
    Ok(())
}