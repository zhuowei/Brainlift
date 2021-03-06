use cranelift_codegen::entity::EntityRef;
use cranelift_codegen::ir::entities::{FuncRef, Value};
use cranelift_codegen::ir::function::Function;
use cranelift_codegen::ir::stackslot::{StackSlotData, StackSlotKind};
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, InstBuilder, MemFlags, Signature,
};
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
    cells_array_addr: Value,
    putchar_fn: FuncRef,
    getchar_fn: FuncRef,
) {
    loop {
        // grab the opcode from the string iterator
        let opcode = match iter.next() {
            Some(opcode) => opcode,
            None => return,
        };
        // define some helper functions
        // emits instructions for moving the Brainf--k index pointer
        let moveptr = |builder: &mut FunctionBuilder, offset: i64| {
            let val = builder.use_var(index_var);
            let tmp = builder.ins().iconst(I32, offset);
            let newval = builder.ins().iadd(val, tmp);
            builder.def_var(index_var, newval);
        };
        let arith = |builder: &mut FunctionBuilder, offset: i64| {
            // read the cell contents: cells_array_addr[index_var]
            let index_val = builder.use_var(index_var);
            // first, calculate cells_array_addr[index_var]
            // (we can skip this with load_complex,
            // but store_complex fails with "Store must have an encoding")
            let index_val_i64 = builder.ins().uextend(I64, index_val);
            let addr = builder.ins().iadd(cells_array_addr, index_val_i64);
            let val = builder.ins().load(I8, MemFlags::trusted(), addr, 0);
            // add the offset to it
            let tmp = builder.ins().iconst(I8, offset);
            let newval = builder.ins().iadd(val, tmp);
            // and store it back
            builder.ins().store(MemFlags::trusted(), newval, addr, 0);
        };
        let mut handle_loop = |builder: &mut FunctionBuilder| {
            // create some new basic blocks.
            // the basic block holding the instructions in the loop
            let inside_block = builder.create_ebb();
            // the basic block holding the instructions outside the loop
            let outside_block = builder.create_ebb();

            // make our current block jump to this new block
            builder.ins().jump(inside_block, &[]);

            // emit the while loop's conditional:
            builder.switch_to_block(inside_block);
            // while (cells_array_addr[index_var] != 0)

            // same as above: grab cells_array_addr[index_var]
            let index_val = builder.use_var(index_var);
            let index_val_i64 = builder.ins().uextend(I64, index_val);
            let addr = builder.ins().iadd(cells_array_addr, index_val_i64);
            let val = builder.ins().load(I8, MemFlags::trusted(), addr, 0);
            // sign extend it
            let val_i32 = builder.ins().sextend(I32, val);

            // exit loop if value is zero
            builder.ins().brz(val_i32, outside_block, &[]);

            // recursively call ourself to generate instructions
            // inside the loop
            emit(
                builder,
                iter,
                index_var,
                cells_array_addr,
                putchar_fn,
                getchar_fn,
            );
            // emit a jump back to the conditional at the end of the loop
            builder.ins().jump(inside_block, &[]);

            // ok, we're done the loop.
            // future instructions will be emitted into the outside block
            builder.switch_to_block(outside_block);
        };
        let handle_print = |builder: &mut FunctionBuilder| {
            // read the cell contents, again
            let index_val = builder.use_var(index_var);
            let index_val_i64 = builder.ins().uextend(I64, index_val);
            let addr = builder.ins().iadd(cells_array_addr, index_val_i64);
            let val = builder.ins().load(I8, MemFlags::trusted(), addr, 0);
            // call putchar
            builder.ins().call(putchar_fn, &[val]);
        };
        let handle_get = |builder: &mut FunctionBuilder| {
            // call getchar
            let inst = builder.ins().call(getchar_fn, &[]);
            let results = builder.inst_results(inst);
            let newval = results[0];
            // write result to cell contents
            let index_val = builder.use_var(index_var);
            let index_val_i64 = builder.ins().uextend(I64, index_val);
            let addr = builder.ins().iadd(cells_array_addr, index_val_i64);
            builder.ins().store(MemFlags::trusted(), newval, addr, 0);
        };
        // switch on the opcode
        match opcode {
            '>' => moveptr(builder, 1),
            '<' => moveptr(builder, -1),
            '+' => arith(builder, 1),
            '-' => arith(builder, -1),
            '[' => handle_loop(builder),
            ']' => return,
            '.' => handle_print(builder),
            ',' => handle_get(builder),
            _ => (),
        };
    }
}

// note: the return type really should have a better error type!
fn compile() -> ModuleResult<()> {
    // first, we construct a TargetIsa to tell Cranelift to emit code for the x86_64 cpu on my computer.
    // https://docs.rs/cranelift-codegen/0.30.0/cranelift_codegen/isa/index.html
    let mut shared_builder = settings::builder();
    shared_builder.enable("is_pic");
    // turn off stack probing: https://github.com/bjorn3/rustc_codegen_cranelift/blob/master/src/lib.rs#L241
    // since we don't implement ___cranelift_probestack
    shared_builder.set("probestack_enabled", "false");
    let shared_flags = settings::Flags::new(shared_builder);
    let isa_builder = isa::lookup(triple!("x86_64-apple-darwin")).unwrap();
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

        // import the putchar function.
        // see FunctionBuilder::call_memset for an example of importing.
        // Same way we defined the signature for "main" above.
        let mut putchar_signature = Signature::new(CallConv::SystemV);
        putchar_signature.params.push(AbiParam::new(I8));
        let putchar_name =
            module.declare_function("putchar", Linkage::Import, &putchar_signature)?;
        let putchar_sigref = builder.import_signature(putchar_signature);
        // according to Cranelift's source,
        // "Function identifiers are namespace 0 in `ir::ExternalName`".
        let putchar_fn = builder.import_function(ExtFuncData {
            name: ExternalName::from(putchar_name),
            signature: putchar_sigref,
            colocated: false,
        });
        // import the getchar function.
        let mut getchar_signature = Signature::new(CallConv::SystemV);
        getchar_signature.returns.push(AbiParam::new(I8));
        let getchar_name =
            module.declare_function("getchar", Linkage::Import, &getchar_signature)?;
        let getchar_sigref = builder.import_signature(getchar_signature);
        let getchar_fn = builder.import_function(ExtFuncData {
            name: ExternalName::from(getchar_name),
            signature: getchar_sigref,
            colocated: false,
        });

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
        // Start inserting instructions into the basic block
        builder.switch_to_block(ebb);
        // initialize the index pointer to 0
        let zero_const = builder.ins().iconst(I32, 0);
        builder.def_var(index_var, zero_const);
        // grab the stack slot's value
        let cells_array_addr = builder.ins().stack_addr(I64, stack_slot, 0);
        // emit some bf
        // https://esolangs.org/wiki/Hello_world_program_in_esoteric_languages#Brainfuck
        let bf_prog = "+[-[<<[+[--->]-[<<<]]]>>>-]>-.---.>..>.<<<<-.<+.>>>>>.>.<<.<-.";
        emit(
            &mut builder,
            &mut (bf_prog.chars()),
            index_var,
            cells_array_addr,
            putchar_fn,
            getchar_fn,
        );
        let return_value = builder.use_var(index_var);
        // add a "return" instruction to return the index variable
        builder.ins().return_(&[return_value]);
        // all blocks' in-edges are known by now.
        builder.seal_all_blocks();
        builder.finalize();
    }

    // now we create a context, and put our function into it
    // the context will lower our function to machine code.
    let mut function_context = Context::for_function(function);
    module.define_function(function_id, &mut function_context)?;
    let file = File::create("out.o").unwrap();
    let product = module.finish();
    // Write the product to a file.
    product.write(file).unwrap();
    Ok(())
}
