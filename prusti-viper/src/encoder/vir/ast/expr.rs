// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fmt;
use std::mem;
use encoder::vir::ast::*;
use std::ops::Mul;

#[derive(Debug, Clone)]
pub enum Expr {
    /// A local var
    Local(LocalVar, Position),
    /// A field access
    Field(Box<Expr>, Field, Position),
    /// The inverse of a `val_ref` field access
    AddrOf(Box<Expr>, Type, Position),
    LabelledOld(String, Box<Expr>, Position),
    Const(Const, Position),
    MagicWand(Box<Expr>, Box<Expr>, Position),
    /// PredicateAccessPredicate: predicate_name, args, frac
    PredicateAccessPredicate(String, Vec<Expr>, Frac, Position),
    FieldAccessPredicate(Box<Expr>, Frac, Position),
    UnaryOp(UnaryOpKind, Box<Expr>, Position),
    BinOp(BinOpKind, Box<Expr>, Box<Expr>, Position),
    // Unfolding: predicate name, predicate_args, in_expr
    Unfolding(String, Vec<Expr>, Box<Expr>, Frac, Position),
    // Cond: guard, then_expr, else_expr
    Cond(Box<Expr>, Box<Expr>, Box<Expr>, Position),
    // ForAll: variables, triggers, body
    ForAll(Vec<LocalVar>, Vec<Trigger>, Box<Expr>, Position),
    // let variable == (expr) in body
    LetExpr(LocalVar, Box<Expr>, Box<Expr>, Position),
    /// FuncApp: function_name, args, formal_args, return_type, Viper position
    FuncApp(String, Vec<Expr>, Vec<LocalVar>, Type, Position),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOpKind {
    Not, Minus
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOpKind {
    EqCmp, GtCmp, GeCmp, LtCmp, LeCmp, Add, Sub, Mul, Div, Mod, And, Or, Implies
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Const {
    Bool(bool),
    Null,
    Int(i64),
    BigInt(String),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Local(ref v, ref _pos) => write!(f, "{}", v),
            Expr::Field(ref base, ref field, ref _pos) => write!(f, "{}.{}", base, field),
            Expr::AddrOf(ref base, _, ref _pos) => write!(f, "&({})", base),
            Expr::Const(ref value, ref _pos) => write!(f, "{}", value),
            Expr::BinOp(op, ref left, ref right, ref _pos) => write!(f, "({}) {} ({})", left, op, right),
            Expr::UnaryOp(op, ref expr, ref _pos) => write!(f, "{}({})", op, expr),
            Expr::PredicateAccessPredicate(ref pred_name, ref args, perm, ref _pos) => write!(
                f, "acc({}({}), {})",
                pred_name,
                args.iter().map(|x| x.to_string()).collect::<Vec<String>>().join(", "),
                perm
            ),
            Expr::FieldAccessPredicate(ref expr, perm, ref _pos) => write!(f, "acc({}, {})", expr, perm),
            Expr::LabelledOld(ref label, ref expr, ref _pos) => write!(f, "old[{}]({})", label, expr),
            Expr::MagicWand(ref left, ref right, ref _pos) => write!(f, "({}) --* ({})", left, right),
            Expr::Unfolding(ref pred_name, ref args, ref expr, frac, ref _pos) => if *frac == Frac::one() {
                write!(
                    f, "(unfolding {}({}) in {})",
                    pred_name,
                    args.iter().map(|x| x.to_string()).collect::<Vec<String>>().join(", "),
                    expr
                )
            } else {
                write!(
                    f, "(unfolding acc({}({}), {}) in {})",
                    pred_name,
                    args.iter().map(|x| x.to_string()).collect::<Vec<String>>().join(", "),
                    frac,
                    expr
                )
            },
            Expr::Cond(ref guard, ref left, ref right, ref _pos) => write!(f, "({})?({}):({})", guard, left, right),
            Expr::ForAll(ref vars, ref triggers, ref body, ref _pos) => write!(
                f, "forall {} {} :: {}",
                vars.iter().map(|x| format!("{:?}", x)).collect::<Vec<String>>().join(", "),
                triggers.iter().map(|x| x.to_string()).collect::<Vec<String>>().join(", "),
                body.to_string()
            ),
            Expr::LetExpr(ref var, ref expr, ref body, ref _pos) => write!(
                f, "(let {:?} == ({}) in {})",
                var,
                expr.to_string(),
                body.to_string()
            ),
            Expr::FuncApp(ref name, ref args, ..) => write!(
                f, "{}({})",
                name,
                args.iter().map(|f| f.to_string()).collect::<Vec<String>>().join(", ")
            ),
        }
    }
}

impl fmt::Display for UnaryOpKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &UnaryOpKind::Not => write!(f, "!"),
            &UnaryOpKind::Minus => write!(f, "-"),
        }
    }
}

impl fmt::Display for BinOpKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &BinOpKind::EqCmp => write!(f, "=="),
            &BinOpKind::GtCmp => write!(f, ">"),
            &BinOpKind::GeCmp => write!(f, ">="),
            &BinOpKind::LtCmp => write!(f, "<"),
            &BinOpKind::LeCmp => write!(f, "<="),
            &BinOpKind::Add => write!(f, "+"),
            &BinOpKind::Sub => write!(f, "-"),
            &BinOpKind::Mul => write!(f, "*"),
            &BinOpKind::Div => write!(f, "\\"),
            &BinOpKind::Mod => write!(f, "%"),
            &BinOpKind::And => write!(f, "&&"),
            &BinOpKind::Or => write!(f, "||"),
            &BinOpKind::Implies => write!(f, "==>"),
        }
    }
}

impl fmt::Display for Const {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Const::Bool(val) => write!(f, "{}", val),
            &Const::Null => write!(f, "null"),
            &Const::Int(val) => write!(f, "{}", val),
            &Const::BigInt(ref val) => write!(f, "{}", val),
        }
    }
}

impl Expr {
    pub fn pos(&self) -> &Position {
        match self {
            Expr::Local(_, ref p) => p,
            Expr::Field(_, _, ref p) => p,
            Expr::AddrOf(_, _, ref p) => p,
            Expr::Const(_, ref p) => p,
            Expr::LabelledOld(_, _, ref p) => p,
            Expr::MagicWand(_, _, ref p) => p,
            Expr::PredicateAccessPredicate(_, _, _, ref p) => p,
            Expr::FieldAccessPredicate(_, _, ref p) => p,
            Expr::UnaryOp(_, _, ref p) => p,
            Expr::BinOp(_, _, _, ref p) => p,
            Expr::Unfolding(_, _, _, _, ref p) => p,
            Expr::Cond(_, _, _, ref p) => p,
            Expr::ForAll(_, _, _, ref p) => p,
            Expr::LetExpr(_, _, _, ref p) => p,
            Expr::FuncApp(_, _, _, _, _, ref p) => p,
        }
    }

    pub fn set_pos(self, pos: Position) -> Self {
        match self {
            Expr::Local(v, _) => Expr::Local(v, pos),
            Expr::Field(e, f, _) => Expr::Field(e, f, pos),
            Expr::AddrOf(e, t, _) => Expr::AddrOf(e, t, pos),
            Expr::Const(x, _) => Expr::Const(x, pos),
            Expr::LabelledOld(x, y, _) => Expr::LabelledOld(x, y, pos),
            Expr::MagicWand(x, y, _) => Expr::MagicWand(x, y, pos),
            Expr::PredicateAccessPredicate(x, y, z, _) => Expr::PredicateAccessPredicate(x, y, z, pos),
            Expr::FieldAccessPredicate(x, y, _) => Expr::FieldAccessPredicate(x, y, pos),
            Expr::UnaryOp(x, y, _) => Expr::UnaryOp(x, y, pos),
            Expr::BinOp(x, y, z, _) => Expr::BinOp(x, y, z, pos),
            Expr::Unfolding(x, y, z, frac, _) => Expr::Unfolding(x, y, z, frac, pos),
            Expr::Cond(x, y, z, _) => Expr::Cond(x, y, z, pos),
            Expr::ForAll(x, y, z, _) => Expr::ForAll(x, y, z, pos),
            Expr::LetExpr(x, y, z, _) => Expr::LetExpr(x, y, z, pos),
            Expr::FuncApp(x, y, z, k, _) => Expr::FuncApp(x, y, z, k, pos),
        }
    }

    pub fn pred_permission(place: Expr, frac: Frac, pos: Position) -> Option<Self> {
        place.typed_ref_name().map( |pred_name|
            Expr::PredicateAccessPredicate(
                pred_name,
                vec![ place ],
                frac,
                pos,
            )
        )
    }

    pub fn acc_permission(place: Expr, frac: Frac, pos: Position) -> Self {
        Expr::FieldAccessPredicate(
            box place,
            frac,
                pos,
        )
    }

    pub fn labelled_old(label: &str, expr: Expr, pos: Position) -> Self {
        Expr::LabelledOld(label.to_string(), box expr, pos)
    }

    pub fn not(expr: Expr, pos: Position) -> Self {
        Expr::UnaryOp(UnaryOpKind::Not, box expr, pos)
    }

    pub fn minus(expr: Expr, pos: Position) -> Self {
        Expr::UnaryOp(UnaryOpKind::Minus, box expr, pos)
    }

    pub fn gt_cmp(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::GtCmp, box left, box right, pos)
    }

    pub fn ge_cmp(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::GeCmp, box left, box right, pos)
    }

    pub fn lt_cmp(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::LtCmp, box left, box right, pos)
    }

    pub fn le_cmp(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::LeCmp, box left, box right, pos)
    }

    pub fn eq_cmp(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::EqCmp, box left, box right, pos)
    }

    pub fn ne_cmp(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::not(Expr::eq_cmp(left, right, pos.clone()), pos)
    }

    pub fn add(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::Add, box left, box right, pos)
    }

    pub fn sub(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::Sub, box left, box right, pos)
    }

    pub fn mul(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::Mul, box left, box right, pos)
    }

    pub fn div(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::Div, box left, box right, pos)
    }

    pub fn modulo(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::Mod, box left, box right, pos)
    }

    /// Encode Rust reminder. This is *not* Viper modulo.
    pub fn rem(left: Expr, right: Expr, pos: Position) -> Self {
        let abs_right = Expr::ite(
            Expr::ge_cmp(right.clone(), 0.into(), pos.clone()),
            right.clone(),
            Expr::minus(right.clone(), pos.clone()),
            pos.clone()
        );
        Expr::ite(
            Expr::or(
                Expr::ge_cmp(left.clone(), 0.into(), pos.clone()),
                Expr::eq_cmp(
                    Expr::modulo(left.clone(), right.clone(), pos.clone()),
                    0.into(),
                    pos.clone()
                ),
                pos.clone()
            ),
            // positive value or left % right == 0
            Expr::modulo(left.clone(), right.clone(), pos.clone()),
            // negative value
            Expr::sub(Expr::modulo(left, right, pos.clone()), abs_right, pos.clone()),
            pos
        )
    }

    pub fn and(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::And, box left, box right, pos)
    }

    pub fn or(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::Or, box left, box right, pos)
    }

    pub fn xor(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::not(Expr::eq_cmp(left, right, pos.clone()), pos)
    }

    pub fn implies(left: Expr, right: Expr, pos: Position) -> Self {
        Expr::BinOp(BinOpKind::Implies, box left, box right, pos)
    }

    pub fn let_expr(variable: LocalVar, expr: Expr, body: Expr, pos: Position) -> Self {
        Expr::LetExpr(variable, box expr, box body, pos)
    }

    pub fn forall(vars: Vec<LocalVar>, triggers: Vec<Trigger>, body: Expr, pos: Position) -> Self {
        Expr::ForAll(vars, triggers, box body, pos)
    }

    pub fn ite(guard: Expr, left: Expr, right: Expr, pos: Position) -> Self {
        Expr::Cond(box guard, box left, box right, pos)
    }

    pub fn unfolding(place: Expr, expr: Expr, frac: Frac, pos: Position) -> Self {
        Expr::Unfolding(
            place.typed_ref_name().unwrap(),
            vec![ place ],
            box expr,
            frac,
            pos
        )
    }

    pub fn func_app(name: String, args: Vec<Expr>, internal_args: Vec<LocalVar>, return_type: Type, pos: Position) -> Self {
        Expr::FuncApp(
            name,
            args,
            internal_args,
            return_type,
            pos
        )
    }

    pub fn find(&self, sub_target: &Expr) -> bool {
        pub struct ExprFinder<'a> {
            sub_target: &'a Expr,
            found: bool
        }
        impl<'a> ExprWalker for ExprFinder<'a> {
            fn walk(&mut self, expr: &Expr) {
                if expr == self.sub_target || (expr.is_place() && expr.weak_eq(self.sub_target)) {
                    self.found = true;
                } else {
                    default_walk_expr(self, expr)
                }
            }
        }

        let mut finder = ExprFinder {
            sub_target,
            found: false,
        };
        finder.walk(self);
        finder.found
    }

    pub fn explode_place(&self) -> (Expr, Vec<Field>) {
        match self {
            Expr::Field(ref base, ref field, ref pos) => {
                let (base_base, mut fields) = base.explode_place();
                fields.push(field.clone());
                (base_base, fields)
            }
            _ => (self.clone(), vec![])
        }
    }

    // Methods from the old `Place` structure

    pub fn local(local: LocalVar, pos: Position) -> Self {
        Expr::Local(local, pos)
    }

    pub fn field(self, field: Field, pos: Position) -> Self {
        Expr::Field(box self, field, pos)
    }

    pub fn addr_of(self, pos: Position) -> Self {
        let type_name = self.get_type().name();
        Expr::AddrOf(box self, Type::TypedRef(type_name), pos)
    }

    pub fn is_place(&self) -> bool {
        match self {
            &Expr::Local(_, _) => true,
            &Expr::Field(ref base, _, _) |
            &Expr::AddrOf(ref base, _, _) |
            &Expr::LabelledOld(_, ref base, _) |
            &Expr::Unfolding(_, _, ref base, _, _) => base.is_place(),
            _ => false
        }
    }

    pub fn is_simple_place(&self) -> bool {
        match self {
            &Expr::Local(_, _) => true,
            &Expr::Field(ref base, _, _) => base.is_simple_place(),
            _ => false
        }
    }

    /// Only defined for places
    pub fn get_parent(&self) -> Option<Expr> {
        debug_assert!(self.is_place());
        match self {
            &Expr::Local(_, _) => None,
            &Expr::Field(box ref base, _, _) |
            &Expr::AddrOf(box ref base, _, _) => Some(base.clone()),
            &Expr::LabelledOld(_, _, _) => None,
            &Expr::Unfolding(ref name, ref args, box ref base, frac, _) => None,
            ref x => panic!("{}", x),
        }
    }

    pub fn map_parent<F>(self, f: F) -> Expr where F: Fn(Expr) -> Expr {
        match self {
            Expr::Field(box base, field, pos) => Expr::Field(box f(base), field, pos),
            Expr::AddrOf(box base, ty, pos) => Expr::AddrOf(box f(base), ty, pos),
            Expr::LabelledOld(label, box base, pos) => Expr::LabelledOld(label, box f(base), pos),
            Expr::Unfolding(name, args, box base, frac, pos) => Expr::Unfolding(name, args, box f(base), frac, pos),
            _ => self,
        }
    }

    pub fn is_local(&self) -> bool {
        match self {
            &Expr::Local(..) => true,
            _ => false,
        }
    }

    pub fn is_field(&self) -> bool {
        match self {
            &Expr::Field(..) => true,
            _ => false,
        }
    }

    pub fn is_addr_of(&self) -> bool {
        match self {
            &Expr::AddrOf(..) => true,
            _ => false,
        }
    }

    pub fn is_unfolding(&self) -> bool {
        match self {
            &Expr::Unfolding(..) => true,
            _ => false,
        }
    }

    /// Puts an `old[label](..)` around the expression
    pub fn old<S: fmt::Display + ToString>(self, label: S, pos: Position) -> Self {
        match self {
            Expr::Local(..) => {
                /*
                debug!(
                    "Trying to put an old expression 'old[{}](..)' around {}, which is a local variable",
                    label,
                    self
                );
                */
                self
            },
            Expr::LabelledOld(..) => {
                /*
                debug!(
                    "Trying to put an old expression 'old[{}](..)' around {}, which already has a label",
                    label,
                    self
                );
                */
                self
            },
            _ => Expr::LabelledOld(label.to_string(), box self, pos)
        }
    }

    /// Puts the place into an `old[label](..)` expression, if the label is not `None`
    pub fn maybe_old<S: fmt::Display + ToString>(self, label: Option<S>, pos: Position) -> Self {
        match label {
            None => self,
            Some(label) => self.old(label, pos),
        }
    }

    pub fn contains_old_label(&self) -> bool {
        struct OldLabelFinder {
            found: bool
        }
        impl ExprWalker for OldLabelFinder {
            fn walk_labelled_old(&mut self, x: &str, y: &Expr, p: &Position) {
                self.found = true;
            }
        }
        let mut walker = OldLabelFinder{
            found: false
        };
        walker.walk(self);
        walker.found
    }

    pub fn is_old(&self) -> bool {
        self.get_label().is_some()
    }

    pub fn is_curr(&self) -> bool {
        !self.is_old()
    }

    pub fn get_place(&self) -> Option<&Expr> {
        match self {
            Expr::PredicateAccessPredicate(_, ref args, _, _) => Some(&args[0]),
            Expr::FieldAccessPredicate(box ref arg, _, _) => Some(arg),
            _ => None,
        }
    }

    pub fn is_pure(&self) -> bool {
        struct PurityFinder {
            non_pure: bool
        }
        impl ExprWalker for PurityFinder {
            fn walk_predicate_access_predicate(&mut self,x: &str, y: &Vec<Expr>, z: Frac, p: &Position) {
                self.non_pure = true;
            }
            fn walk_field_access_predicate(&mut self, x: &Expr, y: Frac, p: &Position) {
                self.non_pure = true;
            }
        }
        let mut walker = PurityFinder{
            non_pure: false
        };
        walker.walk(self);
        !walker.non_pure
    }

    /// Only defined for places
    pub fn get_base(&self) -> LocalVar {
        debug_assert!(self.is_place());
        match self {
            &Expr::Local(ref var, _) => var.clone(),
            &Expr::LabelledOld(_, ref base, _) |
            &Expr::Unfolding(_, _, ref base, _, _) => base.get_base(),
            _ => self.get_parent().unwrap().get_base(),
        }
    }

    pub fn get_label(&self) -> Option<&String> {
        match self {
            &Expr::LabelledOld(ref label, _, _) => Some(label),
            _ => None,
        }
    }

    /* Moved to the Eq impl
    /// Place equality after type elision
    pub fn weak_eq(&self, other: &Expr) -> bool {
        debug_assert!(self.is_place());
        debug_assert!(other.is_place());
        match (self, other) {
            (
                Expr::Local(ref self_var),
                Expr::Local(ref other_var)
            ) => self_var.weak_eq(other_var),
            (
                Expr::Field(box ref self_base, ref self_field),
                Expr::Field(box ref other_base, ref other_field)
            ) => self_field.weak_eq(other_field) && self_base.weak_eq(other_base),
            (
                Expr::AddrOf(box ref self_base, ref self_typ),
                Expr::AddrOf(box ref other_base, ref other_typ)
            ) => self_typ.weak_eq(other_typ) && self_base.weak_eq(other_base),
            (
                Expr::LabelledOld(ref self_label, box ref self_base),
                Expr::LabelledOld(ref other_label, box ref other_base)
            ) => self_label == other_label && self_base.weak_eq(other_base),
            (
                Expr::Unfolding(ref self_name, ref self_args, box ref self_base, self_frac),
                Expr::Unfolding(ref other_name, ref other_args, box ref other_base, other_frac)
            ) => self_name == other_name && self_frac == other_frac &&
                self_args[0].weak_eq(&other_args[0]) && self_base.weak_eq(other_base),
            _ => false
        }
    }
    */

    pub fn has_proper_prefix(&self, other: &Expr) -> bool {
        debug_assert!(self.is_place());
        debug_assert!(other.is_place());
        self != other && self.has_prefix(other)
    }

    pub fn has_prefix(&self, other: &Expr) -> bool {
        debug_assert!(self.is_place());
        debug_assert!(other.is_place());
        if self == other {
            true
        } else {
            match self.get_parent() {
                Some(parent) => parent.has_prefix(other),
                None => false
            }
        }
    }

    pub fn all_proper_prefixes(&self) -> Vec<Expr> {
        debug_assert!(self.is_place());
        match self.get_parent() {
            Some(parent) => parent.all_prefixes(),
            None => vec![]
        }
    }

    // Returns all prefixes, from the shortest to the longest
    pub fn all_prefixes(&self) -> Vec<Expr> {
        debug_assert!(self.is_place());
        let mut res = self.all_proper_prefixes();
        res.push(self.clone());
        res
    }

    pub fn get_type(&self) -> &Type {
        debug_assert!(self.is_place());
        match self {
            &Expr::Local(LocalVar { ref typ, .. }) |
            &Expr::Field(_, Field { ref typ, .. }) |
            &Expr::AddrOf(_, ref typ) => &typ,
            &Expr::LabelledOld(_, box ref base) |
            &Expr::Unfolding(_, _, box ref base, _) => base.get_type(),
            _ => panic!()
        }
    }

    pub fn typed_ref_name(&self) -> Option<String> {
        match self.get_type() {
            &Type::TypedRef(ref name) => Some(name.clone()),
            _ => None
        }
    }

    pub fn map_labels<F>(self, f: F) -> Self where F: Fn(String) -> Option<String> {
        struct OldLabelReplacer<T: Fn(String) -> Option<String>> {
            f: T
        };
        impl<T: Fn(String) -> Option<String>> ExprFolder for OldLabelReplacer<T> {
            fn fold_labelled_old(&mut self, label: String, base: Box<Expr>, pos: Position) -> Expr {
                match (self.f)(label) {
                    Some(new_label) => base.old(new_label, pos),
                    None => *base
                }
            }
        }
        OldLabelReplacer{
            f
        }.fold(self)
    }

    pub fn replace_place(self, target: &Expr, replacement: &Expr) -> Self {
        debug_assert!(target.is_place());
        //assert_eq!(target.get_type(), replacement.get_type());
        if replacement.is_place() {
            assert!(
                target.get_type().weak_eq(&replacement.get_type()),
                "Cannot substitute '{}' with '{}', because they have incompatible types '{}' and '{}'",
                target,
                replacement,
                target.get_type(),
                replacement.get_type()
            );
        }
        struct PlaceReplacer<'a>{
            target: &'a Expr,
            replacement: &'a Expr
        };
        impl<'a> ExprFolder for PlaceReplacer<'a> {
            fn fold(&mut self, e: Expr) -> Expr {
                if e.is_place() && e == self.target {
                    self.replacement.clone()
                } else {
                    default_fold_expr(self, e)
                }
            }

            fn fold_forall(&mut self, vars: Vec<LocalVar>, triggers: Vec<Trigger>, body: Box<Expr>, pos: Position) -> Expr {
                if vars.contains(&self.target.get_base()) {
                    // Do nothing
                    Expr::ForAll(vars, triggers, body, pos)
                } else {
                    Expr::ForAll(
                        vars,
                        triggers.into_iter().map(|x| x.replace_place(self.target, self.replacement)).collect(),
                        self.fold_boxed(body),
                        pos
                    )
                }
            }
        }
        PlaceReplacer {
            target,
            replacement
        }.fold(self)
    }

    /// Replaces expressions like `old[l5](old[l5](_9.val_ref).foo.bar)`
    /// into `old[l5](_9.val_ref.foo.bar)`
    pub fn remove_redundant_old(self) -> Self {
        struct RedundantOldRemover {
            current_label: Option<String>,
        };
        impl ExprFolder for RedundantOldRemover {
            fn fold_labelled_old(&mut self, label: String, base: Box<Expr>, pos: Position) -> Expr {
                let old_current_label = mem::replace(
                    &mut self.current_label,
                    Some(label.clone())
                );
                let new_base = default_fold_expr(self, *base);
                let new_expr = if Some(label.clone()) == old_current_label {
                    new_base
                } else {
                    new_base.old(label, pos)
                };
                self.current_label = old_current_label;
                new_expr
            }
        }
        RedundantOldRemover {
            current_label: None,
        }.fold(self)
    }

    /// Leaves a conjunction of `acc(..)` expressions
    pub fn filter_perm_conjunction(self) -> Self {
        struct PermConjunctionFilter();
        impl ExprFolder for PermConjunctionFilter {
            fn fold(&mut self, e: Expr) -> Expr {
                match e {
                    f @ Expr::PredicateAccessPredicate(..) => f,
                    f @ Expr::FieldAccessPredicate(..) => f,
                    Expr::BinOp(BinOpKind::And, y, z, p) => self.fold_bin_op(BinOpKind::And, y, z, p),

                    Expr::BinOp(..) |
                    Expr::MagicWand(..) |
                    Expr::Unfolding(..) |
                    Expr::Cond(..) |
                    Expr::UnaryOp(..) |
                    Expr::Const(..) |
                    Expr::Local(..) |
                    Expr::Field(..) |
                    Expr::AddrOf(..) |
                    Expr::LabelledOld(..) |
                    Expr::ForAll(..) |
                    Expr::LetExpr(..) |
                    Expr::FuncApp(..) => true.into(),
                }
            }
        }
        PermConjunctionFilter().fold(self)
    }

    /// Apply the closure to all places in the expression.
    pub fn fold_places<F>(self, f: F) -> Expr
        where
            F: Fn(Expr) -> Expr
    {
        struct PlaceFolder<F>
            where
                F: Fn(Expr) -> Expr
        {
            f: F,
        };
        impl<F> ExprFolder for PlaceFolder<F>
            where
                F: Fn(Expr) -> Expr
        {
            fn fold(&mut self, e: Expr) -> Expr {
                if e.is_place() {
                    (self.f)(e)
                } else {
                    default_fold_expr(self, e)
                }
            }
            // TODO: Handle triggers?
        }
        PlaceFolder {
            f
        }.fold(self)
    }
}

impl Const {
    pub fn is_num(&self) -> bool {
        match self {
            &Const::Bool(..) |
            &Const::Null => false,

            &Const::Int(..) |
            &Const::BigInt(..) => true,
        }
    }
}

pub trait ExprFolder : Sized {
    fn fold(&mut self, e: Expr) -> Expr {
        default_fold_expr(self, e)
    }

    fn fold_boxed(&mut self, e: Box<Expr>) -> Box<Expr> {
        box self.fold(*e)
    }

    fn fold_local(&mut self, v: LocalVar, p: Position) -> Expr {
        Expr::Local(v, p)
    }
    fn fold_field(&mut self, e: Box<Expr>, f: Field, p: Position) -> Expr {
        Expr::Field(self.fold_boxed(e), f, p)
    }
    fn fold_addr_of(&mut self, e: Box<Expr>, t: Type, p: Position) -> Expr {
        Expr::AddrOf(self.fold_boxed(e), t, p)
    }
    fn fold_const(&mut self, x: Const, p: Position) -> Expr {
        Expr::Const(x, p)
    }
    fn fold_labelled_old(&mut self, x: String, y: Box<Expr>, p: Position) -> Expr {
        Expr::LabelledOld(x, self.fold_boxed(y), p)
    }
    fn fold_magic_wand(&mut self, x: Box<Expr>, y: Box<Expr>, p: Position) -> Expr {
        Expr::MagicWand(self.fold_boxed(x), self.fold_boxed(y), p)
    }
    fn fold_predicate_access_predicate(&mut self, x: String, y: Vec<Expr>, z: Frac, p: Position) -> Expr {
        Expr::PredicateAccessPredicate(x, y.into_iter().map(|e| self.fold(e)).collect(), z, p)
    }
    fn fold_field_access_predicate(&mut self, x: Box<Expr>, y: Frac, p: Position) -> Expr {
        Expr::FieldAccessPredicate(self.fold_boxed(x), y, p)
    }
    fn fold_unary_op(&mut self, x: UnaryOpKind, y: Box<Expr>, p: Position) -> Expr {
        Expr::UnaryOp(x, self.fold_boxed(y), p)
    }
    fn fold_bin_op(&mut self, x: BinOpKind, y: Box<Expr>, z: Box<Expr>, p: Position) -> Expr {
        Expr::BinOp(x, self.fold_boxed(y), self.fold_boxed(z), p)
    }
    fn fold_unfolding(&mut self, x: String, y: Vec<Expr>, z: Box<Expr>, frac: Frac, p: Position) -> Expr {
        Expr::Unfolding(x, y.into_iter().map(|e| self.fold(e)).collect(), self.fold_boxed(z), frac, p)
    }
    fn fold_cond(&mut self, x: Box<Expr>, y: Box<Expr>, z: Box<Expr>, p: Position) -> Expr {
        Expr::Cond(self.fold_boxed(x), self.fold_boxed(y), self.fold_boxed(z), p)
    }
    fn fold_forall(&mut self, x: Vec<LocalVar>, y: Vec<Trigger>, z: Box<Expr>, p: Position) -> Expr {
        Expr::ForAll(x, y, self.fold_boxed(z), p)
    }
    fn fold_let_expr(&mut self, x: LocalVar, y: Box<Expr>, z: Box<Expr>, p: Position) -> Expr {
        Expr::LetExpr(x, self.fold_boxed(y), self.fold_boxed(z), p)
    }
    fn fold_func_app(&mut self, x: String, y: Vec<Expr>, z: Vec<LocalVar>, k: Type, p: Position) -> Expr {
        Expr::FuncApp(x, y.into_iter().map(|e| self.fold(e)).collect(), z, k, p)
    }
}

pub fn default_fold_expr<T: ExprFolder>(this: &mut T, e: Expr) -> Expr {
    match e {
        Expr::Local(v, p) => this.fold_local(v, p),
        Expr::Field(e, f, p) => this.fold_field(e, f, p),
        Expr::AddrOf(e, t, p) => this.fold_addr_of(e, t, p),
        Expr::Const(x, p) => this.fold_const(x, p),
        Expr::LabelledOld(x, y, p) => this.fold_labelled_old(x, y, p),
        Expr::MagicWand(x, y, p) => this.fold_magic_wand(x, y, p),
        Expr::PredicateAccessPredicate(x, y, z, p) => this.fold_predicate_access_predicate(x, y, z, p),
        Expr::FieldAccessPredicate(x, y, p) => this.fold_field_access_predicate(x, y, p),
        Expr::UnaryOp(x, y, p) => this.fold_unary_op(x, y, p),
        Expr::BinOp(x, y, z, p) => this.fold_bin_op(x, y, z, p),
        Expr::Unfolding(x, y, z, frac, p) => this.fold_unfolding(x, y, z, frac, p),
        Expr::Cond(x, y, z, p) => this.fold_cond(x, y, z, p),
        Expr::ForAll(x, y, z, p) => this.fold_forall(x, y, z, p),
        Expr::LetExpr(x, y, z, p) => this.fold_let_expr(x, y, z, p),
        Expr::FuncApp(x, y, z, k, p) => this.fold_func_app(x, y, z, k, p),
    }
}

pub trait ExprWalker : Sized {
    fn walk(&mut self, e: &Expr) {
        default_walk_expr(self, e);
    }

    fn walk_local(&mut self, x: &LocalVar, p: &Position) {}
    fn walk_field(&mut self, e: &Expr, f: &Field, p: &Position) {
        self.walk(e);
    }
    fn walk_addr_of(&mut self, e: &Expr, t: &Type, p: &Position) {
        self.walk(e);
    }
    fn walk_const(&mut self, x: &Const, p: &Position) {}
    fn walk_old(&mut self, x: &Expr, p: &Position) {
        self.walk(x);
    }
    fn walk_labelled_old(&mut self, x: &str, y: &Expr, p: &Position) {
        self.walk(y);
    }
    fn walk_magic_wand(&mut self, x: &Expr, y: &Expr, p: &Position) {
        self.walk(x);
        self.walk(y);
    }
    fn walk_predicate_access_predicate(&mut self,x: &str, y: &Vec<Expr>, z: Frac, p: &Position) {
        for e in y {
            self.walk(e);
        }
    }
    fn walk_field_access_predicate(&mut self, x: &Expr, y: Frac, p: &Position) {
        self.walk(x)
    }
    fn walk_unary_op(&mut self, x: UnaryOpKind, y: &Expr, p: &Position) {
        self.walk(y)
    }
    fn walk_bin_op(&mut self, x: BinOpKind, y: &Expr, z: &Expr, p: &Position) {
        self.walk(y);
        self.walk(z);
    }
    fn walk_unfolding(&mut self, x: &str, y: &Vec<Expr>, z: &Expr, frac: Frac, p: &Position) {
        for e in y {
            self.walk(e);
        }
        self.walk(z);
    }
    fn walk_cond(&mut self, x: &Expr, y: &Expr, z: &Expr, p: &Position) {
        self.walk(x);
        self.walk(y);
        self.walk(z);
    }
    fn walk_forall(&mut self, x: &Vec<LocalVar>, y: &Vec<Trigger>, z: &Expr, p: &Position) {
        self.walk(z);
    }
    fn walk_let_expr(&mut self, x: &LocalVar, y: &Expr, z: &Expr, p: &Position) {
        self.walk(y);
        self.walk(z);
    }
    fn walk_func_app(&mut self, x: &str, y: &Vec<Expr>, z: &Vec<LocalVar>, k: &Type, p: &Position) {
        for e in y {
            self.walk(e)
        }
    }
}

pub fn default_walk_expr<T: ExprWalker>(this: &mut T, e: &Expr) {
    match *e {
        Expr::Local(ref v, ref p) => this.walk_local(v, p),
        Expr::Field(ref e, ref f, ref p) => this.walk_field(e, f, p),
        Expr::AddrOf(ref e, ref t, ref p) => this.walk_addr_of(e, t, p),
        Expr::Const(ref x, ref p) => this.walk_const(x, p),
        Expr::LabelledOld(ref x, ref y, ref p) => this.walk_labelled_old(x, y, p),
        Expr::MagicWand(ref x, ref y, ref p) => this.walk_magic_wand(x, y, p),
        Expr::PredicateAccessPredicate(ref x, ref y, z, ref p) => this.walk_predicate_access_predicate(x, y, z, p),
        Expr::FieldAccessPredicate(ref x, y, ref p) => this.walk_field_access_predicate(x, y, p),
        Expr::UnaryOp(x, ref y, ref p) => this.walk_unary_op(x, y, p),
        Expr::BinOp(x, ref y, ref z, ref p) => this.walk_bin_op(x, y, z, p),
        Expr::Unfolding(ref x, ref y, ref z, frac, ref p) => this.walk_unfolding(x, y, z, frac, p),
        Expr::Cond(ref x, ref y, ref z, ref p) => this.walk_cond(x, y, z, p),
        Expr::ForAll(ref x, ref y, ref z, ref p) => this.walk_forall(x, y, z, p),
        Expr::LetExpr(ref x, ref y, ref z, ref p) => this.walk_let_expr(x, y, z, p),
        Expr::FuncApp(ref x, ref y, ref z, ref k, ref p) => this.walk_func_app(x, y, z, k, p),
    }
}

impl <'a> Mul<&'a Frac> for Box<Expr> {
    type Output = Box<Expr>;

    fn mul(self, frac: &'a Frac) -> Box<Expr> {
        Box::new(*self * frac)
    }
}

impl <'a> Mul<&'a Frac> for Expr {
    type Output = Expr;

    fn mul(self, frac: &'a Frac) -> Expr {
        match self {
            Expr::PredicateAccessPredicate(x, y, z, p) => Expr::PredicateAccessPredicate(x, y, z * frac, p),
            Expr::FieldAccessPredicate(x, y, p) => Expr::FieldAccessPredicate(x, y * frac, p),
            Expr::UnaryOp(x, y, p) => Expr::UnaryOp(x, y * frac, p),
            Expr::BinOp(x, y, z, p) => Expr::BinOp(x, y * frac, z * frac, p),
            Expr::Cond(x, y, z, p) => Expr::Cond(x, y * frac, z * frac, p),
            _ => self
        }
    }
}

pub trait ExprIterator {
    /// Conjoin a sequence of expressions into a single expression.
    /// Returns true if the sequence has no elements.
    fn conjoin(&mut self, pos: Position) -> Expr;

    /// Disjoin a sequence of expressions into a single expression.
    /// Returns true if the sequence has no elements.
    fn disjoin(&mut self, pos: Position) -> Expr;
}

impl<T> ExprIterator for T
    where
        T: Iterator<Item = Expr>
{
    fn conjoin(&mut self, pos: Position) -> Expr {
        if let Some(init) = self.next() {
            self.fold(init, |acc, conjunct| Expr::and(acc, conjunct, pos.clone()))
        } else {
            true.into()
        }
    }

    fn disjoin(&mut self, pos: Position) -> Expr {
        if let Some(init) = self.next() {
            self.fold(init, |acc, conjunct| Expr::or(acc, conjunct, pos.clone()))
        } else {
            false.into()
        }
    }
}
