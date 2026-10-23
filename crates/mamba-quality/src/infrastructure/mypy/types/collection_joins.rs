//! Dictionary literal values can share the target's read-only Collection type.
use super::*;
use mamba_language::application::quality_syntax::{
    python_ast::{ExprDict, ExprTuple, Number},
    PythonTarget,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum CollectionFamily {
    String,
    List,
    Dictionary,
    Sequence,
    Collection,
}

#[derive(Clone, Copy)]
struct CollectionJoinValue {
    ty: Type,
    // Only this literal fold carries an empty literal's bottom item bound.
    item: Option<Type>,
}

impl Context {
    // check_lst_expr's slow path solves one T for all variadic arguments.
    // Preserve each computed operand; only direct empty syntax supplies bottom.
    pub(super) fn literal_list_common_type(
        &self,
        items: &[Expr],
        observed: &[Type],
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        if items.len() != observed.len() || !self.list_common_sources(items, environment) {
            return None;
        }
        let mut bounds = Vec::new();
        let mut material = false;
        for (expression, &ty) in items.iter().zip(observed) {
            let bound = match expression {
                Expr::NoneLiteral(_) if ty == Type::None => ty,
                Expr::List(list) if list.elts.is_empty() => {
                    if !matches!(self.node(ty).as_deref(),
                        Some(Node::Container(Container::List, args)) if args.as_slice() == [Type::Any])
                    {
                        return None;
                    }
                    self.intern(Node::Container(Container::List, vec![Type::Never]))
                }
                Expr::Dict(dictionary) if dictionary.items.is_empty() => {
                    if !matches!(self.node(ty).as_deref(),
                        Some(Node::Container(Container::Dict, args))
                            if args.as_slice() == [Type::Any, Type::Any])
                    {
                        return None;
                    }
                    // Its two ambiguous bottom arguments are a proper subtype
                    // of every retained material dictionary. Omit this bound
                    // before unresolved bottoms become Any after annotation failure.
                    continue;
                }
                _ if self.list_common_material_dictionary(ty) => {
                    material = true;
                    ty
                }
                _ => return None,
            };
            bounds.push(bound);
        }
        if !material {
            return None;
        }
        // solve_one/_join_sorted_key sorts lower bounds stably: Union -2,
        // None -1, ordinary instances 0, Overloaded 1. Only the middle two
        // ranks occur in this finite domain. Keep equal-rank source order.
        bounds.sort_by_key(|&ty| if ty == Type::None { -1 } else { 0 });
        let variable = self.native_variable("<list-literal>", 0, "T".into(), Type::Object);
        let arguments: Vec<_> = bounds.into_iter().map(|ty| (variable, ty)).collect();
        let solution = self.solve_call(&[variable], &arguments, None);
        if !solution.unresolved.is_empty()
            || !solution.failed.is_empty()
            || !solution.invalid.is_empty()
            || solution.factory_poly.is_some()
        {
            return None;
        }
        let item = *solution.types.get(&variable)?;
        if !self.list_common_pending_item(item) {
            return None;
        }
        Some(self.intern(Node::Container(Container::List, vec![item])))
    }

    // This is a source/shape proof, not another inference or expression visit.
    pub(in crate::infrastructure::mypy) fn literal_list_common_type_incomplete(
        &self,
        expression: &Expr,
        observed: Type,
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        let Expr::List(list) = expression else {
            return false;
        };
        self.observed_expression_type(expression).is_none()
            && self.list_common_sources(&list.elts, environment)
            && matches!(self.node(observed).as_deref(),
                Some(Node::Container(Container::List, args))
                    if args.len() == 1 && self.list_common_pending_item(args[0]))
    }

    pub(super) fn list_common_sources(
        &self,
        items: &[Expr],
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        // A leading None plus these collection instances keeps the original
        // fast fold a Union. A later None alone cannot prove fast-path failure.
        if !matches!(items.first(), Some(Expr::NoneLiteral(_))) {
            return false;
        }
        let mut none = false;
        let mut list = false;
        let mut dictionary = false;
        let mut material = false;
        for expression in items {
            if self.observed_expression_type(expression).is_some() {
                return false;
            }
            match expression {
                Expr::NoneLiteral(_) => none = true,
                Expr::List(value) if value.elts.is_empty() => list = true,
                Expr::Dict(value) if value.items.is_empty() => dictionary = true,
                _ if self.list_common_dictionary_source(expression, environment) => material = true,
                _ => return false,
            }
        }
        none && list && dictionary && material
    }

    fn list_common_dictionary_source(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        if self.observed_expression_type(expression).is_some() {
            return false;
        }
        match expression {
            Expr::Name(name) => environment
                .get(name.id.as_str())
                .is_some_and(|&ty| self.list_common_material_dictionary(ty)),
            Expr::Dict(dictionary) => dictionary.items.iter().all(|item| item.key.is_some()),
            Expr::BinOp(binary) if binary.op == Operator::BitOr => {
                self.list_common_dictionary_source(&binary.left, environment)
                    && self.list_common_dictionary_source(&binary.right, environment)
            }
            _ => false,
        }
    }

    // Object covers a List[Any] instance through nominal inheritance, without
    // inspecting or accepting its Any item. is_same_type uses two proper subtype
    // checks even for non-simplified unions. Keep the observed dictionary intact.
    fn list_common_material_dictionary(&self, ty: Type) -> bool {
        matches!(self.node(ty).as_deref(), Some(Node::Container(Container::Dict, args))
            if args.len() == 2 && args[0] == Type::Str
                && (self.dictionary_or_flat(args[1])
                    || matches!(self.node(args[1]).as_deref(), Some(Node::Union(items))
                        if items.contains(&Type::Object) && items.iter().all(|&item|
                            self.dictionary_or_flat(item)
                                || matches!(self.node(item).as_deref(),
                                    Some(Node::Container(Container::List, args))
                                        if args.as_slice() == [Type::Any])))))
    }

    fn list_common_pending_item(&self, ty: Type) -> bool {
        let Some(node) = self.node(ty) else {
            return false;
        };
        let Node::Union(items) = node.as_ref() else {
            return false;
        };
        items.contains(&Type::None)
            && items.iter().any(|&item| self.list_common_material_dictionary(item))
            && items.iter().any(|&item| matches!(self.node(item).as_deref(),
                Some(Node::Container(Container::List, args)) if args.as_slice() == [Type::Never]))
            && items.iter().all(|&item| item == Type::None
                || self.list_common_material_dictionary(item)
                || matches!(self.node(item).as_deref(),
                    Some(Node::Container(Container::List, args)) if args.as_slice() == [Type::Never]))
    }

    // tuple_fallback uses a simplified union, not an instance join. Keep this
    // recovery local to literal dictionaries whose bottom provenance is known.
    // The bool reports an unresolved nested empty list, not assignment success.
    pub(in crate::infrastructure::mypy) fn literal_dictionary_tuple_item(
        &self,
        tuple: &ExprTuple,
        observed: Type,
    ) -> Option<(Type, bool)> {
        let node = self.node(observed)?;
        let Node::Tuple(rows) = node.as_ref() else {
            return None;
        };
        if rows.len() != tuple.elts.len() {
            return None;
        }
        let mut members = Vec::new();
        let mut incomplete = false;
        for (expression, &row) in tuple.elts.iter().zip(rows) {
            let Expr::Dict(dictionary) = expression else {
                return None;
            };
            let node = self.node(row)?;
            let Node::Container(Container::Dict, arguments) = node.as_ref() else {
                return None;
            };
            let [key, value] = arguments.as_slice() else {
                return None;
            };
            if dictionary.items.is_empty() {
                if *key != Type::Any || *value != Type::Any {
                    return None;
                }
                // The AST proves two ambiguous bottom arguments. Such a dict
                // is a proper subtype of every retained dictionary below.
                // Remove it before the inferred-variable fallback replaces
                // unresolved bottom arguments with Any.
                continue;
            }
            if *key != Type::Str {
                return None;
            }
            let mut scalar = None;
            let mut empty_list = false;
            for entry in &dictionary.items {
                if !matches!(entry.key.as_ref(), Some(Expr::StringLiteral(_))) {
                    return None;
                }
                let atom = match &entry.value {
                    Expr::StringLiteral(_) => Type::Str,
                    Expr::BytesLiteral(_) => Type::Bytes,
                    Expr::BooleanLiteral(_) => Type::Bool,
                    Expr::NoneLiteral(_) => Type::None,
                    Expr::NumberLiteral(number) => match number.value {
                        Number::Int(_) => Type::Int,
                        Number::Float(_) => Type::Float,
                        Number::Complex { .. } => Type::Complex,
                    },
                    Expr::List(list) if list.elts.is_empty() => {
                        empty_list = true;
                        continue;
                    }
                    _ => return None,
                };
                if scalar.is_some_and(|previous| previous != atom) {
                    return None;
                }
                scalar = Some(atom);
            }
            let supported = match (scalar, empty_list) {
                (Some(atom), false) => *value == atom,
                (None, true) => {
                    let pending = matches!(self.node(*value).as_deref(),
                        Some(Node::Container(Container::List, items))
                            if items.as_slice() == [Type::Any]);
                    incomplete |= pending;
                    pending
                }
                // Reuse the existing literal collection join's source proof:
                // str and a direct [] join to Sequence[str], not Sequence[Any].
                (Some(Type::Str), true) => matches!(self.node(*value).as_deref(),
                    Some(Node::Container(Container::Sequence, items))
                        if items.as_slice() == [Type::Str]),
                _ => false,
            };
            if !supported {
                return None;
            }
            members.push(row);
        }
        if members.is_empty() {
            // No material dictionary alternative can remove the empty rows.
            return None;
        }
        // In this finite domain, distinct dictionaries are not proper subtypes:
        // scalar arguments are invariant, and List[Any] cannot accept a
        // Sequence[str] in the reverse direction. The existing simplifier can
        // therefore remove identical dictionaries without using nested Any to
        // drop a distinct member. No general subtype rule is changed.
        Some((
            self.simplify_inference_union(self.union(members)),
            incomplete,
        ))
    }

    // Extend the literal tuple fallback only when a direct [] supplies the
    // unresolved item. Retain that list in the union after variable recovery.
    pub(in crate::infrastructure::mypy) fn literal_empty_list_dictionary_tuple_item(
        &self,
        expression: &Expr,
        observed: Type,
    ) -> Option<(Type, bool)> {
        if self.observed_expression_type(expression).is_some() {
            return None;
        }
        let Expr::Tuple(tuple) = expression else {
            return None;
        };
        let node = self.node(observed)?;
        let Node::Tuple(rows) = node.as_ref() else {
            return None;
        };
        if rows.len() != tuple.elts.len() {
            return None;
        }
        let mut members = Vec::new();
        let mut empty_list = false;
        let mut material_dictionary = false;
        for (expression, &row) in tuple.elts.iter().zip(rows) {
            if self.observed_expression_type(expression).is_some() {
                return None;
            }
            let node = self.node(row)?;
            match (expression, node.as_ref()) {
                (Expr::List(list), Node::Container(Container::List, items))
                    if list.elts.is_empty() && items.as_slice() == [Type::Any] =>
                {
                    empty_list = true;
                    members.push(row);
                }
                (Expr::Dict(dictionary), Node::Container(Container::Dict, items)) => {
                    let [key, value] = items.as_slice() else {
                        return None;
                    };
                    if dictionary.items.is_empty() {
                        if *key != Type::Any || *value != Type::Any {
                            return None;
                        }
                        // Omit only if a material dictionary below exists.
                        continue;
                    }
                    if *key != Type::Str {
                        return None;
                    }
                    let mut integers = false;
                    let mut strings = false;
                    for entry in &dictionary.items {
                        let Some(key @ Expr::StringLiteral(_)) = entry.key.as_ref() else {
                            return None;
                        };
                        if self.observed_expression_type(key).is_some()
                            || self.observed_expression_type(&entry.value).is_some()
                        {
                            return None;
                        }
                        match &entry.value {
                            Expr::StringLiteral(_) => strings = true,
                            Expr::NumberLiteral(number)
                                if matches!(number.value, Number::Int(_)) =>
                            {
                                integers = true;
                            }
                            _ => return None,
                        }
                    }
                    // Pinned fast_dict_type joins int and str to object.
                    // Validate the computed row; do not replace its arguments.
                    let expected = match (integers, strings) {
                        (true, true) => Type::Object,
                        (true, false) => Type::Int,
                        (false, true) => Type::Str,
                        _ => return None,
                    };
                    if *value != expected {
                        return None;
                    }
                    material_dictionary = true;
                    members.push(row);
                }
                _ => return None,
            }
        }
        if !empty_list || !material_dictionary {
            return None;
        }
        // The retained list and invariant dictionaries are distinct alternatives.
        // Deduplicate exact members only; nested Any is not a subtype proof.
        Some((self.union(members), true))
    }

    // A value union containing object has the same original parameter type as
    // object. Preserve its computed representation and every tuple member.
    pub(in crate::infrastructure::mypy) fn literal_object_dictionary_tuple_item(
        &self,
        expression: &Expr,
        observed: Type,
        environment: &BTreeMap<String, Type>,
    ) -> Option<(Type, bool)> {
        let Expr::Tuple(tuple) = expression else {
            return None;
        };
        if self.observed_expression_type(expression).is_some() {
            return None;
        }
        let node = self.node(observed)?;
        let Node::Tuple(rows) = node.as_ref() else {
            return None;
        };
        if rows.len() != tuple.elts.len() {
            return None;
        }
        let mut members = Vec::new();
        let mut empty_list = false;
        let mut object_source = false;
        for (expression, &row) in tuple.elts.iter().zip(rows) {
            if self.observed_expression_type(expression).is_some() {
                return None;
            }
            let node = self.node(row)?;
            match (expression, node.as_ref()) {
                (Expr::List(list), Node::Container(Container::List, items))
                    if list.elts.is_empty() && items.as_slice() == [Type::Any] =>
                {
                    empty_list = true;
                }
                (Expr::Dict(dictionary), Node::Container(Container::Dict, items)) => {
                    if dictionary.items.is_empty() {
                        if items.as_slice() != [Type::Any, Type::Any] {
                            return None;
                        }
                        // The required object dictionary removes this bottom row.
                        continue;
                    }
                    let [key, value] = items.as_slice() else {
                        return None;
                    };
                    if *key != Type::Str
                        || !self.object_dictionary_literal(dictionary, *value, false)
                    {
                        return None;
                    }
                }
                (_, Node::Container(Container::Dict, items))
                    if items.len() == 2
                        && items[0] == Type::Str
                        && self.tuple_object_dictionary_value(items[1]) =>
                {
                    let name = match expression {
                        Expr::Name(name) if environment.get(name.id.as_str()) == Some(&row) => name,
                        Expr::BinOp(binary) if binary.op == Operator::BitOr => {
                            let (Expr::Name(name), Expr::Dict(dictionary)) =
                                (binary.left.as_ref(), binary.right.as_ref())
                            else {
                                return None;
                            };
                            if self.observed_expression_type(&binary.left).is_some()
                                || self.observed_expression_type(&binary.right).is_some()
                                || !self.object_dictionary_literal(dictionary, Type::Object, true)
                            {
                                return None;
                            }
                            name
                        }
                        _ => return None,
                    };
                    // Only the existing Name type proves the receiver. Its
                    // value and the computed result must both reduce to object
                    // under expandtype.remove_trivial, without an Any wildcard.
                    let receiver = self.node(*environment.get(name.id.as_str())?)?;
                    if !matches!(receiver.as_ref(), Node::Container(Container::Dict, arguments)
                        if arguments.len() == 2 && arguments[0] == Type::Str
                            && self.tuple_object_dictionary_value(arguments[1]))
                    {
                        return None;
                    }
                    object_source = true;
                }
                _ => return None,
            }
            members.push(row);
        }
        (empty_list && object_source).then(|| (self.union(members), true))
    }

    // Object absorbs these instance alternatives independently of a nested
    // list's Any item. Do not treat top-level Any, aliases, or unknowns as bottom.
    fn tuple_object_dictionary_value(&self, value: Type) -> bool {
        value == Type::Object
            || matches!(self.node(value).as_deref(), Some(Node::Union(items))
                if items.contains(&Type::Object) && items.iter().all(|&item|
                    self.dictionary_or_flat(item)
                        || matches!(self.node(item).as_deref(),
                            Some(Node::Container(Container::List, arguments))
                                if arguments.as_slice() == [Type::Any])))
    }

    fn object_dictionary_literal(
        &self,
        dictionary: &ExprDict,
        value: Type,
        contextual: bool,
    ) -> bool {
        let mut integers = false;
        let mut strings = false;
        if dictionary.items.is_empty() {
            return false;
        }
        for entry in &dictionary.items {
            let Some(key @ Expr::StringLiteral(_)) = entry.key.as_ref() else {
                return false;
            };
            if self.observed_expression_type(key).is_some()
                || self.observed_expression_type(&entry.value).is_some()
            {
                return false;
            }
            match &entry.value {
                Expr::StringLiteral(_) => strings = true,
                Expr::NumberLiteral(number) if matches!(number.value, Number::Int(_)) => {
                    integers = true;
                }
                // This value fits object; it supplies no bottom receipt.
                Expr::List(list) if contextual && list.elts.is_empty() => {}
                _ => return false,
            }
        }
        if contextual {
            value == Type::Object
        } else {
            match (integers, strings) {
                (true, true) => value == Type::Object,
                (true, false) => value == Type::Int,
                (false, true) => value == Type::Str,
                _ => false,
            }
        }
    }

    // Only direct empty operands supply bottom bounds here. Observed Any in other
    // expressions is not evidence of an unsolved item type.
    pub(in crate::infrastructure::mypy) fn empty_conditional_collection_join(
        &self,
        expression: &Expr,
        observed: Type,
        target: PythonTarget,
    ) -> Option<Type> {
        let Expr::If(conditional) = expression else {
            return None;
        };
        let family = |expression: &Expr| match expression {
            Expr::List(list) if list.elts.is_empty() => Some(("list", 1)),
            Expr::Dict(dictionary) if dictionary.items.is_empty() => Some(("dict", 2)),
            _ => None,
        };
        let left = family(&conditional.body)?;
        let right = family(&conditional.orelse)?;
        if left.0 == right.0 {
            return None;
        }
        let node = self.node(observed)?;
        let Node::Union(arms) = node.as_ref() else {
            return None;
        };
        if arms.len() != 2 {
            return None;
        }
        for (&arm, (name, arity)) in arms.iter().zip([left, right]) {
            let expected = if name == "list" {
                Container::List
            } else {
                Container::Dict
            };
            if !matches!(self.node(arm).as_deref(), Some(Node::Container(kind, arguments))
                if *kind == expected && arguments.len() == arity
                    && arguments.iter().all(|&argument| argument.is_any()))
            {
                return None;
            }
        }
        let collection = self.literal_collection_class(target)?;
        if self.class(collection).generic.len() != 1 {
            return None;
        }
        for (name, arity) in [left, right] {
            let instance = self.runtime_builtin_class_instance(name, target)?;
            let id = self.class_id(instance)?;
            if self.class(id).generic.len() != arity {
                return None;
            }
            let bottom = self.applied_class(id, vec![Type::Never; arity], false);
            if self.class_arguments_for(bottom, collection)? != vec![Type::Never] {
                return None;
            }
        }
        Some(self.applied_class(collection, vec![Type::Never], false))
    }

    pub(in crate::infrastructure::mypy) fn infer_dictionary_collection_join(
        &self,
        dictionary: &ExprDict,
        target: PythonTarget,
        environment: &BTreeMap<String, Type>,
        expression_type: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        let mut keys = None;
        let mut legacy = None;
        let mut candidate = None;
        for entry in &dictionary.items {
            // Infer only children; inferring this whole dictionary would reenter the callback.
            let key = self.infer_with_calls(entry.key.as_ref()?, environment, expression_type)?;
            let value = self.infer_with_calls(&entry.value, environment, expression_type)?;
            keys = Some(keys.map_or(key, |previous| self.join(previous, key)));
            legacy = Some(legacy.map_or(value, |previous| self.join(previous, value)));
            let value = CollectionJoinValue {
                ty: value,
                item: match (&entry.value, self.node(value).as_deref()) {
                    (Expr::List(list), Some(Node::Container(Container::List, arguments)))
                        if list.elts.is_empty() && arguments.len() == 1 =>
                    {
                        Some(Type::Never)
                    }
                    (Expr::Dict(dictionary), Some(Node::Container(Container::Dict, arguments)))
                        if dictionary.items.is_empty() && arguments.len() == 2 =>
                    {
                        Some(Type::Never)
                    }
                    _ => None,
                },
            };
            candidate = Some(candidate.map_or(value, |previous| {
                self.join_collection_literal_values(previous, value, target)
            }));
        }
        let legacy = legacy.unwrap_or(Type::Any);
        let values = candidate
            .and_then(|value| {
                if legacy == Type::Object
                    && self
                        .canonical_collection_item(value.ty, target)
                        .is_some_and(|item| item != Type::Never)
                {
                    return Some(value.ty);
                }
                // Refine only after the entire literal fold. A later real Any
                // item must remain Any, even if an earlier empty operand had
                // a local bottom bound.
                let item = value.item?;
                if item.is_any() || item == Type::Never {
                    return None;
                }
                matches!(
                    (self.node(legacy).as_deref(), self.node(value.ty).as_deref()),
                    (
                        Some(Node::Container(Container::Sequence, old)),
                        Some(Node::Container(Container::Sequence, candidate)),
                    ) if old.len() == 1 && old[0].is_any()
                        && candidate.len() == 1 && candidate[0].is_any()
                )
                .then(|| self.intern(Node::Container(Container::Sequence, vec![item])))
            })
            .unwrap_or(legacy);
        Some(self.intern(Node::Container(
            Container::Dict,
            vec![keys.unwrap_or(Type::Any), values],
        )))
    }

    fn join_collection_literal_values(
        &self,
        left: CollectionJoinValue,
        right: CollectionJoinValue,
        target: PythonTarget,
    ) -> CollectionJoinValue {
        let joined = self.join(left.ty, right.ty);
        let unchanged = CollectionJoinValue {
            ty: joined,
            item: None,
        };
        let Some((left_family, left_item)) = self.collection_literal_parts(left.ty, target) else {
            return unchanged;
        };
        let Some((right_family, right_item)) = self.collection_literal_parts(right.ty, target)
        else {
            return unchanged;
        };
        // A real Any item stays Any. Only direct empty AST operands supply Never.
        let item = self.join(
            left.item.unwrap_or(left_item),
            right.item.unwrap_or(right_item),
        );
        if (joined == Type::Object && left_family != right_family)
            || left_family == CollectionFamily::Collection
            || right_family == CollectionFamily::Collection
        {
            let Some(id) = self.literal_collection_class(target) else {
                return unchanged;
            };
            return CollectionJoinValue {
                ty: self.applied_class(id, vec![item], false),
                item: None,
            };
        }
        CollectionJoinValue {
            ty: joined,
            // Preserve the old sequence result while carrying local empty bounds.
            // Object has no parts, so a later operand cannot rescue a failed join.
            item: self.collection_literal_parts(joined, target).map(|_| item),
        }
    }

    fn collection_literal_parts(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<(CollectionFamily, Type)> {
        if ty == Type::Str {
            return Some((CollectionFamily::String, Type::Str));
        }
        match self.node(ty)?.as_ref() {
            Node::Container(Container::List, arguments) if arguments.len() == 1 => {
                Some((CollectionFamily::List, arguments[0]))
            }
            Node::Container(Container::Dict, arguments) if arguments.len() == 2 => {
                Some((CollectionFamily::Dictionary, arguments[0]))
            }
            Node::Container(Container::Sequence, arguments) if arguments.len() == 1 => {
                Some((CollectionFamily::Sequence, arguments[0]))
            }
            _ => self
                .canonical_collection_item(ty, target)
                .map(|item| (CollectionFamily::Collection, item)),
        }
    }

    fn literal_collection_class(&self, target: PythonTarget) -> Option<u32> {
        let object = self.standard_collection("typing", "Collection", target)?;
        match self.node(object)?.as_ref() {
            Node::Class(id) => Some(*id),
            _ => None,
        }
    }

    pub(in crate::infrastructure::mypy) fn canonical_collection_item(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Type> {
        let node = self.node(ty)?;
        let Node::AppliedClass {
            id,
            arguments,
            value: false,
        } = node.as_ref()
        else {
            return None;
        };
        if arguments.len() != 1 || *id != self.literal_collection_class(target)? {
            return None;
        }
        Some(arguments[0])
    }
}
