fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/");

    // Respect a caller-provided protoc; fall back to the vendored binary so CI
    // runners without protoc installed still build.
    if std::env::var_os("PROTOC").is_none() {
        let protoc = protoc_bin_vendored::protoc_bin_path()?;
        unsafe {
            std::env::set_var("PROTOC", protoc);
        }
    }

    tonic_prost_build::compile_protos("proto/stream_list.proto")?;
    Ok(())
}
