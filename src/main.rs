use cranelift_codegen::entity::EntityRef;
use cranelift_codegen::ir::entities::StackSlot;
use cranelift_codegen::ir::function::Function;
use cranelift_codegen::ir::stackslot::{StackSlotData, StackSlotKind};
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{AbiParam, Ebb, ExternalName, InstBuilder, Signature, Type};
use cranelift_codegen::isa::{self, CallConv};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_faerie::{FaerieBackend, FaerieBuilder, FaerieTrapCollection};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{Linkage, Module, ModuleResult};
use std::fs::File;
use std::str::Chars;
use std::str::FromStr;
use target_lexicon::triple;
fn main() {
    compile().unwrap();
}

fn emit(
    builder: &mut FunctionBuilder,
    iter: &mut Chars,
    index_var: Variable,
    stack_slot: StackSlot,
) {
    // grab the opcode from the string iterator
    let opcode = match iter.next() {
        Some(opcode) => opcode,
        None => return,
    };
    // define some helper functions
    // emits instructions for moving the Brainf--k index pointer
    let mut moveptr = |offset: i64| {
        let val = builder.use_var(index_var);
        let tmp = builder.ins().iconst(I32, offset);
        let newval = builder.ins().iadd(val, tmp);
        builder.def_var(index_var, newval);
    };
    // switch on the opcode
    match opcode {
        '>' => moveptr(1),
        '<' => moveptr(-1),
        _ => (),
    };
    emit(builder, iter, index_var, stack_slot);
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
        // Define a variable to hold the Brainf--k index pointer.
        let index_var = Variable::new(0);
        builder.declare_var(index_var, I32);
        // Allocate the Brainf--k cells on the stack
        let BF_CELLS_COUNT = 30000;
        let stack_slot_data = StackSlotData::new(StackSlotKind::ExplicitSlot, BF_CELLS_COUNT);
        let stack_slot = builder.create_stack_slot(stack_slot_data);
        // Create a basic block
        let ebb = builder.create_ebb();
        // Seal the block: this means that we've already specified all entry points for this block
        // in this case we only have one block, so we can seal it immediately
        builder.seal_block(ebb);
        // Start inserting instructions into the basic block
        builder.switch_to_block(ebb);
        // initialize the index pointer to 0
        let zero_const = builder.ins().iconst(I32, 0);
        builder.def_var(index_var, zero_const);
        // emit some bf
        emit(&mut builder, &mut (">>><<".chars()), index_var, stack_slot);
        let return_value = builder.use_var(index_var);
        // add a "return" instruction to return the index variable
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
