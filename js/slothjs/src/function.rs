use std::collections::HashSet;
use std::fmt;

use crate::ast;
use crate::error::Exception;
use crate::heap::{
    Heap,
    JSRef,
};
use crate::interpret::Interpretable;
use crate::object::{
    Content,
    Interpreted,
    JSObject,
    JSValue,
    ObjectValue,
};

pub struct CallContext {
    pub this_ref: JSRef,
    pub method_name: String,
    pub arguments: Vec<Interpreted>,
}

impl CallContext {
    pub fn execute(self, func_ref: JSRef, heap: &mut Heap) -> Result<Interpreted, Exception> {
        // Yes, we do need a clone() to workaround borrow checker:
        match &heap.get(func_ref).value {
            ObjectValue::VMCall(vmcall) => vmcall.clone().call(self, heap),
            ObjectValue::Closure(closure) => closure.clone().call(self, heap),
            _ => {
                let callee = Interpreted::Member {
                    of: self.this_ref,
                    name: self.method_name,
                };
                return Err(Exception::TypeErrorNotCallable(callee));
            }
        }
    }
}

pub type NativeFunction =
    fn(ctx: CallContext, heap: &'_ mut Heap) -> Result<Interpreted, Exception>;

/// A wrapper for NativeFunction to give it `fmt::Debug`.
#[derive(Clone)]
pub struct VMCall(NativeFunction);

impl VMCall {
    pub fn from_func(f: NativeFunction) -> VMCall {
        VMCall(f)
    }

    pub fn call(self, call: CallContext, heap: &mut Heap) -> Result<Interpreted, Exception> {
        self.0(call, heap)
    }
    pub fn ptr(&self) -> usize {
        self.0 as *const () as usize
    }
}

impl fmt::Debug for VMCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VMCall(*{:x})", self.ptr())
    }
}

#[derive(Clone, Debug)]
pub struct Closure {
    pub id: Option<ast::Identifier>,
    pub params: Vec<ast::Identifier>, // cannot be a set, needs order
    pub variables: HashSet<ast::Identifier>,
    pub body: ast::BlockStatement,
    pub captured_scope: JSRef, // TODO: capture free variables only
}

impl Closure {
    pub fn call(self, call: CallContext, heap: &mut Heap) -> Result<Interpreted, Exception> {
        let result = heap.enter_new_scope(call.this_ref, self.captured_scope, |heap, scoperef| {
            // `arguments`
            let argv = (call.arguments.iter())
                .map(|v| v.to_value(heap))
                .collect::<Result<Vec<JSValue>, Exception>>()?;
            let arguments_ref = heap.alloc(JSObject::from_array(argv));
            heap.get_mut(scoperef)
                .set_nonconf("arguments", Content::from(arguments_ref))?;

            // set each argument
            for (i, param) in self.params.iter().enumerate() {
                let value = (call.arguments.get(i))
                    .unwrap_or(&Interpreted::VOID)
                    .to_value(heap)?;
                heap.get_mut(scoperef)
                    .set_nonconf(param.as_str(), Content::Value(value))?;
            }
            // TODO: save caller site into the new scope

            heap.declare_variables(&self.variables)?;

            self.body.interpret(heap)
        });
        match result {
            Ok(_) => Ok(Interpreted::VOID), // BlockStatement result
            Err(Exception::JumpReturn(returned)) => Ok(returned),
            Err(e) => Err(e),
        }
    }
}
