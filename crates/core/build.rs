use std::io::Result;

fn main() -> Result<()> {
    let proto_root = "../../proto";
    let protos = &[
        format!("{proto_root}/offbeat/v1/types.proto"),
        format!("{proto_root}/offbeat/v1/gossip.proto"),
        format!("{proto_root}/offbeat/v1/relay.proto"),
    ];

    prost_build::Config::new()
        .out_dir("src/proto")
        .compile_protos(protos, &[proto_root])?;

    // Re-run if any proto file changes
    for proto in protos {
        println!("cargo:rerun-if-changed={proto}");
    }

    Ok(())
}
