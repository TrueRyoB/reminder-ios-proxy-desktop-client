fn main() {
    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path().unwrap());
    config
        .compile_protos(
            &["proto/reminders.proto", "proto/versioned_document.proto"],
            &["proto/"],
        )
        .expect("failed to compile CRDT protobuf definitions");
}
