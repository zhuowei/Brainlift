use cranelift_codegen::{Context};
use cranelift_codegen::ir::{Signature, Type, ExternalName};
use cranelift_codegen::ir::function::{Function};
use cranelift_codegen::isa::{self, CallConv};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_faerie::{FaerieBackend, FaerieBuilder, FaerieTrapCollection};
use cranelift_module::{Linkage, Module, ModuleResult};
use std::str::FromStr;
use target_lexicon::triple;
fn main() {
    compile().unwrap();
}

// note: the return type really should have a better error type!
fn compile() -> ModuleResult<()> {
    // first, we construct a TargetIsa to tell Cranelift to emit code for the x86_64 cpu on my computer.
    // https://docs.rs/cranelift-codegen/0.30.0/cranelift_codegen/isa/index.html
    let mut shared_builder = settings::builder();
    shared_builder.enable("is_pic");
    let shared_flags = settings::Flags::new(shared_builder);
    let mut isa_builder = isa::lookup(triple!("x86_64-apple-darwin")).unwrap();
    let isa = isa_builder.finish(shared_flags);
    // To emit some code, we need a Backend to write the code to an object file
    // so we can link it into an executable.
    // we use the cranelift_faerie crate for this.
    let backend_builder = FaerieBuilder::new(
        isa,
        "out.obj".to_string(),
        FaerieTrapCollection::Disabled,
        cranelift_module::default_libcall_names(),
    )?;
    let mut module: Module<FaerieBackend> = Module::new(backend_builder);
    // define our main function
    // note: this gives void main(), which is wrong but it still works.
    let mut signature = Signature::new(CallConv::SystemV);
    let function_id = module.declare_function("main", Linkage::Export, &signature)?;
    // let's actually generate some code now.
    // we create a Function, which holds our target independent code.
    // TODO: how to get a proper external name?!
    let mut function = Function::with_name_signature(ExternalName::user(0, 0), signature);
    // TODO: actually put in some code
    // now we create a context, and put our function into it
    // the context will lower our function to machine code.
    let mut function_context = Context::for_function(function);
    module.define_function(function_id, &mut function_context)?;
    Ok(())
}
