use std::collections::HashMap;

use crate::ast::*;
use crate::env::Env;
use crate::value::*;

/// Evaluate `class Foo { ... }` — create a class definition.
pub(super) fn eval_class_def(
    name: &str,
    parent: &Option<String>,
    mixins: &[String],
    members: &[MemberDef],
    env: &Env,
) -> Result<Value, EvalError> {
    let mut fields = Vec::new();
    let mut methods = HashMap::new();
    let mut constructor = None;
    let mut transforms = Vec::new();

    for member in members {
        match member {
            MemberDef::Field {
                name: field_name,
                type_hint,
                default,
            } => {
                fields.push(ClassField {
                    name: field_name.clone(),
                    type_hint: type_hint.clone(),
                    default: default.clone(),
                });
            }
            MemberDef::Method {
                name: method_name,
                params,
                body,
                ..
            } => {
                let body_expr = match body {
                    MethodBody::Expr(e) => e.clone(),
                    MethodBody::Block(stmts) => Expr::Sequence(stmts.clone()),
                };
                methods.insert(
                    method_name.clone(),
                    ClassMethod {
                        name: method_name.clone(),
                        params: params.clone(),
                        body: body_expr,
                    },
                );
            }
            MemberDef::Constructor { params, body } => {
                let body_expr = if body.len() == 1 {
                    body[0].clone()
                } else {
                    Expr::Sequence(body.clone())
                };
                constructor = Some(ClassConstructor {
                    params: params.clone(),
                    body: body_expr,
                });
            }
            MemberDef::Transform { name: _, rules } => {
                transforms.extend(rules.clone());
            }
        }
    }

    let parent_name = parent.clone();
    if let Some(ref parent_name) = parent_name
        && let Some(parent_val) = env.get(parent_name)
        && let Value::Class(parent_class) = parent_val
    {
        let child_field_names: std::collections::HashSet<String> =
            fields.iter().map(|f| f.name.clone()).collect();
        for parent_field in &parent_class.fields {
            if !child_field_names.contains(&parent_field.name) {
                fields.insert(0, parent_field.clone());
            }
        }
        for (method_name, method) in &parent_class.methods {
            methods
                .entry(method_name.clone())
                .or_insert_with(|| method.clone());
        }
        if constructor.is_none() {
            constructor = parent_class.constructor.clone();
        }
    }

    for mixin_name in mixins {
        if let Some(mixin_val) = env.get(mixin_name)
            && let Value::Class(mixin_class) = mixin_val
        {
            for (method_name, method) in &mixin_class.methods {
                methods
                    .entry(method_name.clone())
                    .or_insert_with(|| method.clone());
            }
        }
    }

    let class_def = ClassDef {
        name: name.to_string(),
        parent: parent_name,
        mixins: mixins.to_vec(),
        fields,
        methods,
        constructor,
        transforms,
    };
    let class_val = Value::Class(std::sync::Arc::new(class_def));
    env.set(name.to_string(), class_val);
    Ok(Value::Null)
}
