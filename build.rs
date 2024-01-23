

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // panic!("BUILD.RS main running.");
    tonic_build::compile_protos("protos/scow.proto")?;
    Ok(())
 }