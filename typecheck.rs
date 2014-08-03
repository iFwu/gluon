use std::collections::HashMap;
use ast::*;
use interner::*;



type TcIdent = InternedStr;
type TcType = Type<TcIdent>;

#[deriving(Show)]
enum TypeError {
    UndefinedVariable(TcIdent),
    NotAFunction(TcType),
    WrongNumberOfArguments(uint, uint),
    TypeMismatch(TcType, TcType),
    UndefinedStruct(TcIdent),
    UndefinedField(TcIdent, TcIdent),
    TypeError(&'static str)
}

type TcResult = Result<TcType, TypeError>;


fn find_type<'a>(module: &'a Module<TcIdent>, name: &TcIdent) -> Option<TcType> {
    module.functions.iter()
        .find(|f| f.name == *name)
        .map(|f| FunctionType(f.arguments.iter().map(|field| field.typ.clone()).collect(), box f.return_type.clone()))
        .or_else(|| module.structs.iter()
            .find(|s| s.name == *name)
            .map(|s| FunctionType(s.fields.iter().map(|field| field.typ.clone()).collect(), box Type(s.name.clone())))
        )
}

pub struct Typecheck<'a> {
    module: &'a Module<TcIdent>,
    stack: HashMap<TcIdent, TcType>
}

impl <'a> Typecheck<'a> {
    
    pub fn new(module: &'a Module<TcIdent>) -> Typecheck<'a> {
        Typecheck { module: module, stack: HashMap::new() }
    }
    
    fn find(&self, id: &TcIdent) -> TcResult {
        self.stack.find(id)
            .map(|t| t.clone())
            .or_else(|| find_type(self.module, id))
            .map(Ok)
            .unwrap_or_else(|| Err(UndefinedVariable(id.clone())))
    }

    fn find_struct(&self, id: &TcIdent) -> Result<&'a Struct<TcIdent>, TypeError> {
        self.module.structs.iter()
            .find(|s| s.name == *id)
            .map(|s| Ok(s))
            .unwrap_or_else(|| Err(UndefinedStruct(id.clone())))
    }

    fn stack_var(&mut self, id: TcIdent, typ: TcType) {
        self.stack.insert(id, typ);
    }

    pub fn typecheck_module(&mut self, module: &Module<TcIdent>) -> Result<(), TypeError> {
        for f in module.functions.iter() {
            try!(self.typecheck_function(f));
        }
        Ok(())
    }

    fn typecheck_function(&mut self, function: &Function<TcIdent>) -> Result<(), TypeError> {
        self.stack.clear();
        for arg in function.arguments.iter() {
            self.stack_var(arg.name.clone(), arg.typ.clone());
        }
        let return_type = try!(self.typecheck(&function.expression));
        self.unify(&function.return_type, return_type)
            .map(|_| ())
    }

    fn typecheck(&mut self, expr: &Expr<TcIdent>) -> TcResult {
        match *expr {
            Identifier(ref id) => self.find(id),
            Literal(ref lit) => {
                Ok(match *lit {
                    Integer(_) => int_type.clone(),
                    Float(_) => float_type.clone(),
                    String(_) => string_type.clone(),
                    Bool(_) => bool_type.clone()
                })
            }
            Call(ref func, ref args) => {
                let func_type = try!(self.typecheck(&**func));
                match func_type {
                    FunctionType(arg_types, return_type) => {
                        if arg_types.len() != args.len() {
                            return Err(WrongNumberOfArguments(arg_types.len(), args.len()));
                        }
                        for i in range(0, arg_types.len()) {
                            let actual = try!(self.typecheck(&args[i]));
                            try!(self.unify(&arg_types[i], actual));
                        }
                        Ok(*return_type)
                    }
                    t => Err(NotAFunction(t))
                }
            }
            IfElse(ref pred, ref if_true, ref if_false) => {
                let pred_type = try!(self.typecheck(&**pred));
                self.unify(&bool_type, pred_type);
                let true_type = try!(self.typecheck(&**if_true));
                let false_type = try!(self.typecheck(&**if_false));
                self.unify(&true_type, false_type)
            }
            While(ref pred, ref expr) => {
                let pred_type = try!(self.typecheck(&**pred));
                try!(self.unify(&bool_type, pred_type));
                self.typecheck(&**expr)
                    .map(|_| unit_type.clone())
            }
            BinOp(ref lhs, ref op, ref rhs) => {
                let lhs_type = try!(self.typecheck(&**lhs));
                let rhs_type = try!(self.typecheck(&**rhs));
                try!(self.unify(&lhs_type, rhs_type.clone()));
                match op.as_slice() {
                    "+" | "-" | "*" => {
                        if lhs_type == int_type || lhs_type == float_type {
                            try!(self.unify(&int_type, rhs_type));
                        }
                        else if rhs_type == int_type || rhs_type == float_type {
                            try!(self.unify(&int_type, lhs_type.clone()));
                        }
                        else {
                            return Err(TypeError("Expected numbers in "))
                        }
                        Ok(lhs_type)
                    }
                    "<" | ">" | "<=" | ">=" => Ok(bool_type.clone()),
                    _ => Err(UndefinedVariable(op.clone()))
                }
            }
            Block(ref exprs) => {
                let mut typ = unit_type.clone();
                for expr in exprs.iter() {
                    typ = try!(self.typecheck(expr));
                }
                for expr in exprs.iter() {
                    match expr {
                        &Let(ref id, _) => {
                            self.stack.remove(id);
                        }
                        _ => ()
                    }
                }
                Ok(typ)
            }
            Match(ref expr, ref alts) => {
                let typ = try!(self.typecheck(&**expr));
                try!(self.typecheck_pattern(&alts[0].pattern, typ.clone()));
                let alt1_type = try!(self.typecheck(&alts[0].expression));
                for alt in alts.iter().skip(1) {
                    try!(self.typecheck_pattern(&alt.pattern, typ.clone()));
                    let alt_type = try!(self.typecheck(&alts[0].expression));
                    try!(self.unify(&alt1_type, alt_type));
                }
                Ok(alt1_type)
            }
            Let(ref id, ref expr) => {
                let typ = try!(self.typecheck(&**expr));
                self.stack_var(id.clone(), typ);
                Ok(unit_type.clone())
            }
            Assign(ref lhs, ref rhs) => {
                let rhs_type = try!(self.typecheck(&**rhs));
                let lhs_type = try!(self.typecheck(&**lhs));
                self.unify(&lhs_type, rhs_type)
            }
            FieldAccess(ref expr, ref id) => {
                let typ = try!(self.typecheck(&**expr));
                match typ {
                    Type(ref struct_id) => {
                        let s = try!(self.find_struct(struct_id));
                        s.fields.iter()
                            .find(|f| f.name == *id)
                            .map(|f| Ok(f.typ.clone()))
                            .unwrap_or_else(|| Err(UndefinedField(struct_id.clone(), id.clone())))
                    }
                    FunctionType(..) => Err(TypeError("Field access on function")),
                    LiteralType(..) => Err(TypeError("Field acces on primitive"))
                }
            }
        }
    }

    fn typecheck_pattern(&mut self, pattern: &Pattern<TcIdent>, match_type: TcType) -> Result<(), TypeError> {
        match *pattern {
            ConstructorPattern(ref name, _) => {
                let ctor_type = try!(self.find(name));
                match ctor_type {
                    FunctionType(_, ref return_type) => {
                        self.unify(&**return_type, match_type)
                            .map(|_| ())
                    }
                    _ => self.unify(&ctor_type, match_type)
                            .map(|_| ())
                }
            }
            _ => Ok(())
        }
    }

    fn unify(&self, expected: &TcType, actual: TcType) -> TcResult {
        if *expected == actual {
            Ok(actual)
        }
        else {
            Err(TypeMismatch(expected.clone(), actual))
        }
    }
}

pub trait Typed {
    fn type_of(&self) -> &Type<InternedStr>;
}
impl <Id: Typed + Str> Typed for Expr<Id> {
    fn type_of(&self) -> &Type<InternedStr> {
        match *self {
            Identifier(ref id) => id.type_of(),
            Literal(ref lit) => {
                match *lit {
                    Integer(_) => &int_type,
                    Float(_) => &float_type,
                    String(_) => &string_type,
                    Bool(_) => &bool_type
                }
            }
            IfElse(_, ref arm, _) => arm.type_of(),
            Block(ref exprs) => {
                if exprs.len() == 0 {
                    &unit_type
                }
                else {
                    exprs.last().unwrap().type_of()
                }
            }
            BinOp(ref lhs, ref op, _) => {
                match op.as_slice() {
                    "+" | "-" | "*" => lhs.type_of(),
                    "<" | ">" | "<=" | ">=" => &bool_type,
                    _ => fail!()
                }
            }
            Let(..) | While(..) | Assign(..) => &unit_type,
            Call(ref func, _) => {
                match func.type_of() {
                    &FunctionType(_, ref return_type) => &**return_type,
                    _ => fail!()
                }
            }
            Match(_, ref alts) => alts[0].expression.type_of(),
            FieldAccess(_, ref id) => id.type_of()
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use parser::*;
    use parser::tests::*;

    #[test]
    fn while_() {
        let text = "fn main() { let x = 2; while x < 10 { x } }";
        let module = parse(text, |p| p.module());
        let mut tc = Typecheck::new(&module);
        tc.typecheck_module(&module)
            .unwrap_or_else(|err| fail!(err))

    }
}