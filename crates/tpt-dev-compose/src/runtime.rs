//! Instantiation and execution of linked core modules.
//!
//! [`Runtime::instantiate`] takes a [`Registry`] and the [`Plan`] that links
//! it, and creates an instance of every module in the plan's order. A
//! function import is bound to the function it names in its provider, and a
//! global import to the global. Each module's start function runs once the
//! module is instantiated. [`Runtime::invoke`] then calls an exported
//! function.
//!
//! The runtime runs a subset of WebAssembly, and it runs no other.
//!
//! - Value types are `i32` and `i64`. Floats, vectors, references and
//!   multi-value block types are refused.
//! - Instructions are control flow (`block`, `loop`, `if`, `br`, `br_if`,
//!   `return`, `call`), `unreachable`, `nop`, `drop`, `select`, local and
//!   global access, integer constants, the integer comparison, arithmetic,
//!   bitwise and shift operations, and the `i32.wrap_i64`, `i64.extend_i32_s`
//!   and `i64.extend_i32_u` conversions. Any other instruction, such as a
//!   memory access or `call_indirect`, is refused when its module is
//!   instantiated, so no module runs partly.
//! - Memories and tables are refused, whether defined or imported. Tags are
//!   refused as imports. A mutable global can be defined, but not imported.
//! - Module code is not validated beyond what decoding checks: indices are in
//!   range, branches name labels that exist, and types are checked as the code
//!   runs. A value of the wrong type traps.
//! - Execution is bounded. Each invocation has a fuel budget, one unit for each
//!   instruction, and a call depth limit. Either one stops execution with a
//!   [`Trap`].
//!
//! Components are not instantiated. Component-level linking is only planned.
//!
//! This is an interpreter, not a sandbox. It does not check the code of the
//! modules it runs beyond what is described here.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::interp::{self, Body, Op, Reject, Scope};
use crate::link::{Plan, Registry};
use crate::module::{Export, ExportKind, FuncType, ImportDesc, ValType};

/// A runtime value. Only the two integer types are supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    /// A 32-bit integer.
    I32(i32),
    /// A 64-bit integer.
    I64(i64),
}

impl Value {
    /// The value's type.
    ///
    /// ```
    /// use tpt_dev_compose::Value;
    /// use tpt_dev_compose::module::ValType;
    ///
    /// assert_eq!(Value::I64(7).ty(), ValType::I64);
    /// ```
    pub fn ty(self) -> ValType {
        match self {
            Value::I32(_) => ValType::I32,
            Value::I64(_) => ValType::I64,
        }
    }
}

/// Why execution stopped before the function returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trap {
    /// An `unreachable` instruction ran.
    Unreachable,
    /// An integer division or remainder had a zero divisor.
    DivideByZero,
    /// A signed division overflowed, as in `i32::MIN / -1`.
    IntegerOverflow,
    /// The calls nested deeper than the limit.
    CallDepthExceeded,
    /// The invocation used up its fuel budget.
    OutOfFuel,
    /// An instruction needed a value that was not on the stack.
    StackUnderflow,
    /// An instruction received a value of the wrong type. Modules are not
    /// validated, so this is how a type error in the code shows.
    TypeMismatch,
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Trap::Unreachable => "an unreachable instruction ran",
            Trap::DivideByZero => "integer division by zero",
            Trap::IntegerOverflow => "integer overflow in a signed division",
            Trap::CallDepthExceeded => "the call depth limit was exceeded",
            Trap::OutOfFuel => "the fuel budget was used up",
            Trap::StackUnderflow => "an instruction found too few values on the stack",
            Trap::TypeMismatch => "an instruction received a value of the wrong type",
        })
    }
}

/// A feature of a module that the runtime does not support.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    /// A memory or a table, defined or imported.
    MemoryOrTable,
    /// An imported tag.
    Tag,
    /// A mutable imported global.
    MutableGlobalImport,
    /// A value type other than `i32` and `i64`.
    NonIntegerType,
    /// A block type with more than one result, or a type index.
    MultiValue,
    /// An instruction, by its opcode, outside the supported subset.
    Opcode(u8),
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Feature::MemoryOrTable => f.write_str("a memory or a table"),
            Feature::Tag => f.write_str("an imported tag"),
            Feature::MutableGlobalImport => f.write_str("a mutable imported global"),
            Feature::NonIntegerType => f.write_str("a value type other than i32 and i64"),
            Feature::MultiValue => f.write_str("a multi-value block type"),
            Feature::Opcode(op) => write!(f, "the instruction with opcode {op:#04x}"),
        }
    }
}

/// Why a module could not be instantiated, or why an invocation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    /// The module uses a feature the runtime does not support.
    Unsupported {
        /// The module.
        module: String,
        /// The feature.
        feature: Feature,
    },
    /// The module's code or constant expressions are malformed.
    Malformed {
        /// The module.
        module: String,
        /// What is wrong.
        reason: &'static str,
    },
    /// The module's start function trapped.
    StartTrapped {
        /// The module.
        module: String,
        /// The trap.
        trap: Trap,
    },
    /// No instance has the given name.
    UnknownInstance(String),
    /// The instance has no exported function with the given name.
    UnknownExport {
        /// The instance.
        instance: String,
        /// The export name.
        name: String,
    },
    /// The arguments do not match the function's parameter types or count.
    ArgumentMismatch,
    /// Execution trapped.
    Trap(Trap),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::Unsupported { module, feature } => {
                write!(
                    f,
                    "module `{module}` uses {feature}, which the runtime does not support"
                )
            }
            RuntimeError::Malformed { module, reason } => {
                write!(f, "module `{module}` is malformed: {reason}")
            }
            RuntimeError::StartTrapped { module, trap } => {
                write!(f, "the start function of module `{module}` trapped: {trap}")
            }
            RuntimeError::UnknownInstance(name) => write!(f, "no instance named `{name}`"),
            RuntimeError::UnknownExport { instance, name } => {
                write!(f, "instance `{instance}` has no exported function `{name}`")
            }
            RuntimeError::ArgumentMismatch => {
                f.write_str("the arguments do not match the function's parameters")
            }
            RuntimeError::Trap(trap) => write!(f, "execution trapped: {trap}"),
        }
    }
}

impl core::error::Error for RuntimeError {}

/// The fuel budget of each invocation, unless [`Runtime::set_fuel`] changes it.
pub const DEFAULT_FUEL: u64 = 10_000_000;

/// The deepest that calls may nest.
const MAX_DEPTH: usize = 64;

/// One instance of a module.
#[derive(Debug)]
struct Instance {
    name: String,
    /// The function index space of the instance, as runtime function ids.
    func_map: Vec<usize>,
    /// The global index space of the instance, as runtime global ids.
    global_map: Vec<usize>,
    exports: Vec<Export>,
}

/// A defined function: its instance, its type and its decoded body.
#[derive(Debug)]
struct Func {
    instance: usize,
    ty: FuncType,
    body: Body,
}

/// A label on the control stack of a running function.
#[derive(Clone, Copy, Debug)]
struct Label {
    /// The instruction a branch to this label continues at.
    cont: usize,
    /// The values a branch to this label carries.
    arity: usize,
    /// The stack height when the label was entered.
    height: usize,
    /// Whether the label is a loop, which a branch re-enters with no values.
    is_loop: bool,
}

/// A set of instantiated modules, ready to run.
///
/// ```
/// use tpt_dev_compose::{Registry, Runtime, Value, parse};
///
/// // (module (func (export "add") (param i32 i32) (result i32)
/// //   local.get 0 local.get 1 i32.add))
/// let bytes = [
///     0, 0x61, 0x73, 0x6D, 1, 0, 0, 0,
///     1, 7, 1, 0x60, 2, 0x7F, 0x7F, 1, 0x7F,
///     3, 2, 1, 0,
///     7, 7, 1, 3, b'a', b'd', b'd', 0, 0,
///     10, 9, 1, 7, 0, 0x20, 0, 0x20, 1, 0x6A, 0x0B,
/// ];
/// let module = parse(&bytes).unwrap();
/// let mut registry = Registry::new();
/// registry.register("math", &module).unwrap();
/// let plan = registry.link().unwrap();
/// let mut runtime = Runtime::instantiate(&registry, &plan).unwrap();
///
/// let out = runtime.invoke("math", "add", &[Value::I32(2), Value::I32(40)]).unwrap();
/// assert_eq!(out, [Value::I32(42)]);
/// ```
#[derive(Debug)]
pub struct Runtime {
    instances: Vec<Instance>,
    funcs: Vec<Func>,
    globals: Vec<Value>,
    fuel: u64,
}

fn reject(module: &str, r: Reject) -> RuntimeError {
    match r {
        Reject::Unsupported(feature) => RuntimeError::Unsupported {
            module: String::from(module),
            feature,
        },
        Reject::Malformed(reason) => RuntimeError::Malformed {
            module: String::from(module),
            reason,
        },
    }
}

fn malformed(module: &str, reason: &'static str) -> RuntimeError {
    RuntimeError::Malformed {
        module: String::from(module),
        reason,
    }
}

/// Refuses the features that the runtime does not run.
fn check_supported(name: &str, module: &crate::module::Module) -> Result<(), RuntimeError> {
    let unsupported = |feature| RuntimeError::Unsupported {
        module: String::from(name),
        feature,
    };
    if !module.memories.is_empty() || !module.tables.is_empty() {
        return Err(unsupported(Feature::MemoryOrTable));
    }
    for import in &module.imports {
        match &import.desc {
            ImportDesc::Table(_) | ImportDesc::Memory(_) => {
                return Err(unsupported(Feature::MemoryOrTable));
            }
            ImportDesc::Tag(_) => return Err(unsupported(Feature::Tag)),
            ImportDesc::Global(g) if g.mutable => {
                return Err(unsupported(Feature::MutableGlobalImport));
            }
            _ => {}
        }
    }
    let integer = |t: &ValType| matches!(t, ValType::I32 | ValType::I64);
    for ty in &module.types {
        if !ty.params.iter().all(integer) || !ty.results.iter().all(integer) {
            return Err(unsupported(Feature::NonIntegerType));
        }
    }
    if module.globals.iter().any(|g| !integer(&g.ty)) {
        return Err(unsupported(Feature::NonIntegerType));
    }
    Ok(())
}

impl Runtime {
    /// Instantiates every module of `plan` in its order, and runs each start
    /// function as its module is instantiated.
    ///
    /// `plan` must be the plan of `registry`. A module that uses a feature the
    /// runtime does not support is refused, and so is any module whose code
    /// does not decode.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::{Feature, Registry, Runtime, RuntimeError, parse};
    ///
    /// // An empty module instantiates.
    /// let empty = parse(&[0, 0x61, 0x73, 0x6D, 1, 0, 0, 0]).unwrap();
    /// let mut registry = Registry::new();
    /// registry.register("empty", &empty).unwrap();
    /// let plan = registry.link().unwrap();
    /// assert!(Runtime::instantiate(&registry, &plan).is_ok());
    ///
    /// // A module with a memory is refused as a whole, so nothing runs partly.
    /// let memory = parse(&[0, 0x61, 0x73, 0x6D, 1, 0, 0, 0, 5, 3, 1, 0, 1]).unwrap();
    /// let mut registry = Registry::new();
    /// registry.register("mem", &memory).unwrap();
    /// let plan = registry.link().unwrap();
    /// let err = Runtime::instantiate(&registry, &plan).unwrap_err();
    /// assert!(matches!(
    ///     err,
    ///     RuntimeError::Unsupported { feature: Feature::MemoryOrTable, .. }
    /// ));
    /// ```
    pub fn instantiate(registry: &Registry<'_>, plan: &Plan) -> Result<Self, RuntimeError> {
        let modules = &registry.modules;
        let mut rt = Runtime {
            instances: Vec::new(),
            funcs: Vec::new(),
            globals: Vec::new(),
            fuel: DEFAULT_FUEL,
        };

        // For each module and each of its imports, the provider module and the export.
        let mut resolved: Vec<Vec<Option<(usize, usize)>>> = modules
            .iter()
            .map(|(_, m)| vec![None; m.imports.len()])
            .collect();
        for b in &plan.bindings {
            if let Some(slot) = resolved
                .get_mut(b.importer)
                .and_then(|v| v.get_mut(b.import))
            {
                *slot = Some((b.provider, b.export));
            }
        }

        let mut instance_of: Vec<Option<usize>> = vec![None; modules.len()];
        for &m in &plan.order {
            let (name, module) = modules
                .get(m)
                .map(|(n, module)| (n.as_str(), *module))
                .ok_or(malformed(
                    "",
                    "the plan names a module that is not registered",
                ))?;
            check_supported(name, module)?;

            let mut func_map = Vec::new();
            let mut global_map = Vec::new();
            for (k, import) in module.imports.iter().enumerate() {
                let (p, e) =
                    resolved[m][k].ok_or(malformed(name, "an import is not bound by the plan"))?;
                let provider = instance_of[p]
                    .map(|i| &rt.instances[i])
                    .ok_or(malformed(name, "a provider is not instantiated yet"))?;
                let export = modules[p].1.exports.get(e).ok_or(malformed(
                    name,
                    "a binding names an export that does not exist",
                ))?;
                let index = export.index as usize;
                match import.desc {
                    ImportDesc::Func(_) => func_map.push(*provider.func_map.get(index).ok_or(
                        malformed(name, "a binding names a function that does not exist"),
                    )?),
                    ImportDesc::Global(_) => {
                        global_map.push(*provider.global_map.get(index).ok_or(malformed(
                            name,
                            "a binding names a global that does not exist",
                        ))?)
                    }
                    _ => {
                        return Err(RuntimeError::Unsupported {
                            module: String::from(name),
                            feature: Feature::MemoryOrTable,
                        });
                    }
                }
            }

            // The mutability of every global in the index space: imports, then definitions.
            let mut globals_mutable: Vec<bool> = module
                .imports
                .iter()
                .filter_map(|i| match &i.desc {
                    ImportDesc::Global(g) => Some(g.mutable),
                    _ => None,
                })
                .collect();
            globals_mutable.extend(module.globals.iter().map(|g| g.mutable));

            // The values of the globals so far: imported first, then each definition.
            let mut values: Vec<Value> = global_map.iter().map(|&g| rt.globals[g]).collect();
            for (i, gt) in module.globals.iter().enumerate() {
                let init = module
                    .global_inits
                    .get(i)
                    .ok_or(malformed(name, "a global has no initialiser"))?;
                let value =
                    interp::const_expr(init, gt.ty, &values).map_err(|r| reject(name, r))?;
                global_map.push(rt.globals.len());
                rt.globals.push(value);
                values.push(value);
            }

            let scope = Scope {
                funcs: module.func_count(),
                globals_mutable: &globals_mutable,
            };
            let index = rt.instances.len();
            for (j, &type_index) in module.functions.iter().enumerate() {
                let ty = module
                    .types
                    .get(type_index as usize)
                    .ok_or(malformed(name, "a function has no type"))?
                    .clone();
                let code = module
                    .code
                    .get(j)
                    .ok_or(malformed(name, "a defined function has no body"))?;
                let body = interp::decode_body(code, ty.params.len(), &scope)
                    .map_err(|r| reject(name, r))?;
                func_map.push(rt.funcs.len());
                rt.funcs.push(Func {
                    instance: index,
                    ty,
                    body,
                });
            }

            rt.instances.push(Instance {
                name: String::from(name),
                func_map,
                global_map,
                exports: module.exports.clone(),
            });
            instance_of[m] = Some(index);

            if let Some(start) = module.start {
                let fid = *rt.instances[index]
                    .func_map
                    .get(start as usize)
                    .ok_or(malformed(name, "the start function does not exist"))?;
                let ty = &rt.funcs[fid].ty;
                if !ty.params.is_empty() || !ty.results.is_empty() {
                    return Err(malformed(
                        name,
                        "the start function takes or returns values",
                    ));
                }
                let mut fuel = rt.fuel;
                rt.call(fid, Vec::new(), &mut fuel, 0).map_err(|trap| {
                    RuntimeError::StartTrapped {
                        module: String::from(name),
                        trap,
                    }
                })?;
            }
        }
        Ok(rt)
    }

    /// Sets the fuel each [`invoke`](Self::invoke) may use. Each instruction
    /// costs one unit.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::{Registry, Runtime, RuntimeError, Trap, parse};
    ///
    /// // (module (func $spin (loop br 0)) (export "spin" (func $spin)))
    /// let bytes = [
    ///     0, 0x61, 0x73, 0x6D, 1, 0, 0, 0,
    ///     1, 4, 1, 0x60, 0, 0,
    ///     3, 2, 1, 0,
    ///     7, 8, 1, 4, b's', b'p', b'i', b'n', 0, 0,
    ///     10, 9, 1, 7, 0, 3, 0x40, 0x0C, 0, 0x0B, 0x0B,
    /// ];
    /// let module = parse(&bytes).unwrap();
    /// let mut registry = Registry::new();
    /// registry.register("spin", &module).unwrap();
    /// let plan = registry.link().unwrap();
    /// let mut runtime = Runtime::instantiate(&registry, &plan).unwrap();
    ///
    /// runtime.set_fuel(1_000);
    /// // The loop never ends, so the call stops when the fuel is gone.
    /// let err = runtime.invoke("spin", "spin", &[]).unwrap_err();
    /// assert!(matches!(err, RuntimeError::Trap(Trap::OutOfFuel)));
    /// ```
    pub fn set_fuel(&mut self, fuel: u64) {
        self.fuel = fuel;
    }

    /// The fuel each [`invoke`](Self::invoke) may use.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_dev_compose::{Registry, Runtime, parse};
    ///
    /// let empty = parse(&[0, 0x61, 0x73, 0x6D, 1, 0, 0, 0]).unwrap();
    /// let mut registry = Registry::new();
    /// registry.register("empty", &empty).unwrap();
    /// let plan = registry.link().unwrap();
    /// let mut runtime = Runtime::instantiate(&registry, &plan).unwrap();
    ///
    /// assert_eq!(runtime.fuel(), 10_000_000); // the default budget
    /// runtime.set_fuel(5);
    /// assert_eq!(runtime.fuel(), 5);
    /// ```
    pub fn fuel(&self) -> u64 {
        self.fuel
    }

    /// Calls the function exported as `export` by the instance named `instance`.
    ///
    /// ```
    /// use tpt_dev_compose::{Registry, Runtime, Value, parse};
    ///
    /// let module = parse(&[0, b'a', b's', b'm', 1, 0, 0, 0]).unwrap();
    /// let mut registry = Registry::new();
    /// registry.register("empty", &module).unwrap();
    /// let plan = registry.link().unwrap();
    /// let mut runtime = Runtime::instantiate(&registry, &plan).unwrap();
    /// assert!(runtime.invoke("empty", "missing", &[]).is_err());
    /// ```
    pub fn invoke(
        &mut self,
        instance: &str,
        export: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, RuntimeError> {
        let idx = self
            .instances
            .iter()
            .position(|i| i.name == instance)
            .ok_or_else(|| RuntimeError::UnknownInstance(String::from(instance)))?;
        let inst = &self.instances[idx];
        let fid = inst
            .exports
            .iter()
            .find(|e| e.kind == ExportKind::Func && e.name == export)
            .and_then(|e| inst.func_map.get(e.index as usize).copied())
            .ok_or_else(|| RuntimeError::UnknownExport {
                instance: String::from(instance),
                name: String::from(export),
            })?;
        let params = &self.funcs[fid].ty.params;
        if args.len() != params.len() || args.iter().zip(params).any(|(v, t)| v.ty() != *t) {
            return Err(RuntimeError::ArgumentMismatch);
        }
        let mut fuel = self.fuel;
        self.call(fid, args.to_vec(), &mut fuel, 0)
            .map_err(RuntimeError::Trap)
    }

    /// Runs function `fid` with `args`, and returns its results.
    fn call(
        &mut self,
        fid: usize,
        args: Vec<Value>,
        fuel: &mut u64,
        depth: usize,
    ) -> Result<Vec<Value>, Trap> {
        if depth >= MAX_DEPTH {
            return Err(Trap::CallDepthExceeded);
        }
        let func = &self.funcs[fid];
        let instance = func.instance;
        let results = func.ty.results.len();
        let code_len = func.body.code.len();
        let mut locals = args;
        locals.extend(func.body.locals.iter().map(|t| match t {
            ValType::I64 => Value::I64(0),
            _ => Value::I32(0),
        }));

        let mut stack: Vec<Value> = Vec::new();
        // The function itself is the outermost label. A branch to it returns.
        let mut labels = vec![Label {
            cont: code_len,
            arity: results,
            height: 0,
            is_loop: false,
        }];
        let mut pc = 0usize;
        while pc < code_len {
            if *fuel == 0 {
                return Err(Trap::OutOfFuel);
            }
            *fuel -= 1;
            let op = self.funcs[fid].body.code[pc];
            pc += 1;
            match op {
                Op::Unreachable => return Err(Trap::Unreachable),
                Op::Nop => {}
                Op::Block { end, results } => labels.push(Label {
                    cont: end + 1,
                    arity: results,
                    height: stack.len(),
                    is_loop: false,
                }),
                Op::Loop => labels.push(Label {
                    cont: pc - 1,
                    arity: 0,
                    height: stack.len(),
                    is_loop: true,
                }),
                Op::If { els, end, results } => {
                    let cond = pop_i32(&mut stack)?;
                    labels.push(Label {
                        cont: end + 1,
                        arity: results,
                        height: stack.len(),
                        is_loop: false,
                    });
                    if cond == 0 {
                        // Without an else, the end pops the label.
                        pc = els.map_or(end, |e| e + 1);
                    }
                }
                Op::Else { end } => pc = end,
                Op::End => {
                    if labels.len() > 1 {
                        labels.pop();
                    }
                }
                Op::Br(depth) => pc = branch(&mut stack, &mut labels, depth)?,
                Op::BrIf(depth) => {
                    if pop_i32(&mut stack)? != 0 {
                        pc = branch(&mut stack, &mut labels, depth)?;
                    }
                }
                Op::Return => {
                    // The function's own label is the outermost one.
                    let outermost = labels.len() - 1;
                    pc = branch(&mut stack, &mut labels, outermost)?;
                }
                Op::Call(f) => {
                    let target = self.instances[instance].func_map[f];
                    let n = self.funcs[target].ty.params.len();
                    if stack.len() < n {
                        return Err(Trap::StackUnderflow);
                    }
                    let args = stack.split_off(stack.len() - n);
                    let out = self.call(target, args, fuel, depth + 1)?;
                    stack.extend(out);
                }
                Op::Drop => {
                    pop(&mut stack)?;
                }
                Op::Select => {
                    let cond = pop_i32(&mut stack)?;
                    let b = pop(&mut stack)?;
                    let a = pop(&mut stack)?;
                    stack.push(if cond != 0 { a } else { b });
                }
                Op::LocalGet(i) => stack.push(locals[i]),
                Op::LocalSet(i) => locals[i] = pop(&mut stack)?,
                Op::LocalTee(i) => {
                    let v = *stack.last().ok_or(Trap::StackUnderflow)?;
                    locals[i] = v;
                }
                Op::GlobalGet(i) => {
                    stack.push(self.globals[self.instances[instance].global_map[i]])
                }
                Op::GlobalSet(i) => {
                    let v = pop(&mut stack)?;
                    let g = self.instances[instance].global_map[i];
                    self.globals[g] = v;
                }
                Op::Const(v) => stack.push(v),
                Op::Numeric(op) => numeric(op, &mut stack)?,
            }
        }

        if stack.len() < results {
            return Err(Trap::StackUnderflow);
        }
        Ok(stack.split_off(stack.len() - results))
    }
}

/// Takes a branch to the label `depth` levels out. It keeps the label's
/// values, drops the stack above the label's height, and leaves the labels
/// that the branch exits. It returns where execution continues.
fn branch(stack: &mut Vec<Value>, labels: &mut Vec<Label>, depth: usize) -> Result<usize, Trap> {
    let idx = labels
        .len()
        .checked_sub(depth + 1)
        .ok_or(Trap::StackUnderflow)?;
    let label = labels[idx];
    let arity = if label.is_loop { 0 } else { label.arity };
    if stack.len() < arity {
        return Err(Trap::StackUnderflow);
    }
    let carried = stack.split_off(stack.len() - arity);
    stack.truncate(label.height);
    stack.extend(carried);
    labels.truncate(idx);
    Ok(label.cont)
}

fn pop(stack: &mut Vec<Value>) -> Result<Value, Trap> {
    stack.pop().ok_or(Trap::StackUnderflow)
}

fn pop_i32(stack: &mut Vec<Value>) -> Result<i32, Trap> {
    match pop(stack)? {
        Value::I32(v) => Ok(v),
        Value::I64(_) => Err(Trap::TypeMismatch),
    }
}

fn pop_i64(stack: &mut Vec<Value>) -> Result<i64, Trap> {
    match pop(stack)? {
        Value::I64(v) => Ok(v),
        Value::I32(_) => Err(Trap::TypeMismatch),
    }
}

/// Runs a numeric or conversion instruction. The opcodes are the ones
/// [`interp`] accepts.
fn numeric(op: u8, stack: &mut Vec<Value>) -> Result<(), Trap> {
    match op {
        // i32 eqz and the comparisons, which return an i32.
        0x45 => {
            let a = pop_i32(stack)?;
            stack.push(Value::I32(i32::from(a == 0)));
        }
        0x46..=0x4F => {
            let b = pop_i32(stack)?;
            let a = pop_i32(stack)?;
            stack.push(Value::I32(i32::from(cmp32(op - 0x46, a, b))));
        }
        // i64 eqz and the comparisons, which return an i32.
        0x50 => {
            let a = pop_i64(stack)?;
            stack.push(Value::I32(i32::from(a == 0)));
        }
        0x51..=0x5A => {
            let b = pop_i64(stack)?;
            let a = pop_i64(stack)?;
            stack.push(Value::I32(i32::from(cmp64(op - 0x51, a, b))));
        }
        0x67..=0x69 => {
            let a = pop_i32(stack)?;
            let r = match op {
                0x67 => a.leading_zeros(),
                0x68 => a.trailing_zeros(),
                _ => a.count_ones(),
            };
            stack.push(Value::I32(r as i32));
        }
        0x6A..=0x78 => {
            let b = pop_i32(stack)?;
            let a = pop_i32(stack)?;
            stack.push(Value::I32(bin32(op, a, b)?));
        }
        0x79..=0x7B => {
            let a = pop_i64(stack)?;
            let r = match op {
                0x79 => a.leading_zeros(),
                0x7A => a.trailing_zeros(),
                _ => a.count_ones(),
            };
            stack.push(Value::I64(i64::from(r)));
        }
        0x7C..=0x8A => {
            let b = pop_i64(stack)?;
            let a = pop_i64(stack)?;
            stack.push(Value::I64(bin64(op, a, b)?));
        }
        0xA7 => {
            let a = pop_i64(stack)?;
            stack.push(Value::I32(a as i32));
        }
        0xAC => {
            let a = pop_i32(stack)?;
            stack.push(Value::I64(i64::from(a)));
        }
        0xAD => {
            let a = pop_i32(stack)?;
            stack.push(Value::I64(i64::from(a as u32)));
        }
        _ => return Err(Trap::TypeMismatch),
    }
    Ok(())
}

/// The comparison `k`, in the order of the i32 comparison opcodes after eqz.
fn cmp32(k: u8, a: i32, b: i32) -> bool {
    let (ua, ub) = (a as u32, b as u32);
    match k {
        0 => a == b,
        1 => a != b,
        2 => a < b,
        3 => ua < ub,
        4 => a > b,
        5 => ua > ub,
        6 => a <= b,
        7 => ua <= ub,
        8 => a >= b,
        _ => ua >= ub,
    }
}

/// The comparison `k`, in the order of the i64 comparison opcodes after eqz.
fn cmp64(k: u8, a: i64, b: i64) -> bool {
    let (ua, ub) = (a as u64, b as u64);
    match k {
        0 => a == b,
        1 => a != b,
        2 => a < b,
        3 => ua < ub,
        4 => a > b,
        5 => ua > ub,
        6 => a <= b,
        7 => ua <= ub,
        8 => a >= b,
        _ => ua >= ub,
    }
}

/// A binary i32 instruction. Division traps on a zero divisor, and a signed
/// division traps on overflow, as the specification requires. Shifts and
/// rotations use the count modulo 32.
fn bin32(op: u8, a: i32, b: i32) -> Result<i32, Trap> {
    let (ua, ub) = (a as u32, b as u32);
    let nonzero = || {
        if b == 0 {
            Err(Trap::DivideByZero)
        } else {
            Ok(())
        }
    };
    Ok(match op {
        0x6A => a.wrapping_add(b),
        0x6B => a.wrapping_sub(b),
        0x6C => a.wrapping_mul(b),
        0x6D => {
            nonzero()?;
            if a == i32::MIN && b == -1 {
                return Err(Trap::IntegerOverflow);
            }
            a / b
        }
        0x6E => {
            nonzero()?;
            (ua / ub) as i32
        }
        0x6F => {
            nonzero()?;
            a.wrapping_rem(b)
        }
        0x70 => {
            nonzero()?;
            (ua % ub) as i32
        }
        0x71 => a & b,
        0x72 => a | b,
        0x73 => a ^ b,
        0x74 => a.wrapping_shl(ub),
        0x75 => a.wrapping_shr(ub),
        0x76 => ua.wrapping_shr(ub) as i32,
        0x77 => ua.rotate_left(ub & 31) as i32,
        0x78 => ua.rotate_right(ub & 31) as i32,
        _ => return Err(Trap::TypeMismatch),
    })
}

/// A binary i64 instruction. The rules are those of [`bin32`].
fn bin64(op: u8, a: i64, b: i64) -> Result<i64, Trap> {
    let (ua, ub) = (a as u64, b as u64);
    let nonzero = || {
        if b == 0 {
            Err(Trap::DivideByZero)
        } else {
            Ok(())
        }
    };
    Ok(match op {
        0x7C => a.wrapping_add(b),
        0x7D => a.wrapping_sub(b),
        0x7E => a.wrapping_mul(b),
        0x7F => {
            nonzero()?;
            if a == i64::MIN && b == -1 {
                return Err(Trap::IntegerOverflow);
            }
            a / b
        }
        0x80 => {
            nonzero()?;
            (ua / ub) as i64
        }
        0x81 => {
            nonzero()?;
            a.wrapping_rem(b)
        }
        0x82 => {
            nonzero()?;
            (ua % ub) as i64
        }
        0x83 => a & b,
        0x84 => a | b,
        0x85 => a ^ b,
        0x86 => a.wrapping_shl(ub as u32),
        0x87 => a.wrapping_shr(ub as u32),
        0x88 => ua.wrapping_shr(ub as u32) as i64,
        0x89 => ua.rotate_left((ub & 63) as u32) as i64,
        0x8A => ua.rotate_right((ub & 63) as u32) as i64,
        _ => return Err(Trap::TypeMismatch),
    })
}
