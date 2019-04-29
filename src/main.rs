use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_faerie::{FaerieBackend, FaerieBuilder, FaerieTrapCollection};
use cranelift_module::{Module, ModuleResult};
use std::str::FromStr;
use target_lexicon::triple;
fn main() {
    println!("Hello, world!");
    compile().unwrap();
}

fn compile() -> ModuleResult<()> {
    // first, we construct a TargetIsa to tell Cranelift to emit code for the x86_64 cpu on my computer.
    // https://docs.rs/cranelift-codegen/0.30.0/cranelift_codegen/isa/index.html
    let shared_builder = settings::builder();
    let shared_flags = settings::Flags::new(shared_builder);
    let isa = isa::lookup(triple!("x86_64-apple-darwin"))
        .unwrap()
        .finish(shared_flags);
    // To emit some code, we need a Backend to write the code to an object file
    // so we can link it into an executable.
    // we use the cranelift_faerie crate for this.
    let backend_builder = FaerieBuilder::new(
        isa,
        "out.obj".to_string(),
        FaerieTrapCollection::Disabled,
        FaerieBuilder::default_libcall_names(),
    )?;
    let module: Module<FaerieBackend> = Module::new(backend_builder);
    Ok(())
}
