use cranelift_codegen::{Context};
use cranelift_codegen::ir::{AbiParam, Signature, Type, ExternalName, InstBuilder, Ebb};
use cranelift_codegen::ir::function::{Function};
use cranelift_codegen::ir::types::*;
use cranelift_codegen::isa::{self, CallConv};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_faerie::{FaerieBackend, FaerieBuilder, FaerieTrapCollection};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{Linkage, Module, ModuleResult};
use std::fs::File;
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
    signature.returns.push(AbiParam::new(I32));
    let function_id = module.declare_function("main", Linkage::Export, &signature)?;
    // let's actually generate some code now.
    // we create a Function, which holds our target independent instructions.
    // TODO: how to get a proper external name?!
    let mut function = Function::with_name_signature(ExternalName::user(0, 0), signature);

    // Actually put in some code.
    // See the sample at https://docs.rs/cranelift-frontend/0.37.0/cranelift_frontend/
    // Allocate a function builder context - temporary storage for functions
    let mut function_builder_context = FunctionBuilderContext::new();
    {
        // Allocated a builder, which helps us create the target independent instructions
        let mut builder = FunctionBuilder::new(&mut function, &mut function_builder_context);
        // Create a basic block
        let ebb = builder.create_ebb();
        // Seal the block: this means that we've already specified all entry points for this block
        // in this case we only have one block, so we can seal it immediately
        builder.seal_block(ebb);
        // Start inserting instructions into the basic block
        builder.switch_to_block(ebb);
        // add an "iconst" instruction
        let return_value = builder.ins().iconst(I32, 42);
        // add a "return" instruction to return the constant we just loaded
        builder.ins().return_(&[return_value]);
        builder.finalize();
    }

    // now we create a context, and put our function into it
    // the context will lower our function to machine code.
    let mut function_context = Context::for_function(function);
    module.define_function(function_id, &mut function_context)?;
    let mut file = File::create("out.o").unwrap();
    let product = module.finish();
    // Write the product to a file.
    product.write(file).unwrap();
    Ok(())
}
