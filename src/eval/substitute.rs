use crate::ast::Expr;

pub(crate) fn substitute_in_expr(expr: &Expr, subs: &[(String, Expr)]) -> Expr {
    match expr {
        Expr::Symbol(name) => subs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, replacement)| replacement.clone())
            .unwrap_or_else(|| expr.clone()),
        Expr::Call { head, args } => Expr::Call {
            head: Box::new(substitute_in_expr(head, subs)),
            args: args.iter().map(|a| substitute_in_expr(a, subs)).collect(),
        },
        Expr::List(items) => {
            Expr::List(items.iter().map(|i| substitute_in_expr(i, subs)).collect())
        }
        Expr::Assoc(pairs) => Expr::Assoc(
            pairs
                .iter()
                .map(|(k, v)| (k.clone(), substitute_in_expr(v, subs)))
                .collect(),
        ),
        Expr::Sequence(items) => {
            Expr::Sequence(items.iter().map(|i| substitute_in_expr(i, subs)).collect())
        }
        Expr::Rule { lhs, rhs } => Expr::Rule {
            lhs: Box::new(substitute_in_expr(lhs, subs)),
            rhs: Box::new(substitute_in_expr(rhs, subs)),
        },
        Expr::RuleDelayed { lhs, rhs } => Expr::RuleDelayed {
            lhs: Box::new(substitute_in_expr(lhs, subs)),
            rhs: Box::new(substitute_in_expr(rhs, subs)),
        },
        Expr::Slot(_) => expr.clone(),
        Expr::SlotSequence(_) => expr.clone(),
        Expr::Function { params, body } => Expr::Function {
            params: params.clone(),
            body: Box::new(substitute_in_expr(body, subs)),
        },
        Expr::Pure { body } => Expr::Pure {
            body: Box::new(substitute_in_expr(body, subs)),
        },
        Expr::ReplaceAll { expr: inner, rules } => Expr::ReplaceAll {
            expr: Box::new(substitute_in_expr(inner, subs)),
            rules: Box::new(substitute_in_expr(rules, subs)),
        },
        Expr::ReplaceRepeated { expr: inner, rules } => Expr::ReplaceRepeated {
            expr: Box::new(substitute_in_expr(inner, subs)),
            rules: Box::new(substitute_in_expr(rules, subs)),
        },
        Expr::Map { func, list } => Expr::Map {
            func: Box::new(substitute_in_expr(func, subs)),
            list: Box::new(substitute_in_expr(list, subs)),
        },
        Expr::Apply { func, expr: inner } => Expr::Apply {
            func: Box::new(substitute_in_expr(func, subs)),
            expr: Box::new(substitute_in_expr(inner, subs)),
        },
        Expr::Pipe { expr: inner, func } => Expr::Pipe {
            expr: Box::new(substitute_in_expr(inner, subs)),
            func: Box::new(substitute_in_expr(func, subs)),
        },
        Expr::Prefix { func, arg } => Expr::Prefix {
            func: Box::new(substitute_in_expr(func, subs)),
            arg: Box::new(substitute_in_expr(arg, subs)),
        },
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => Expr::If {
            condition: Box::new(substitute_in_expr(condition, subs)),
            then_branch: Box::new(substitute_in_expr(then_branch, subs)),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(substitute_in_expr(e, subs))),
        },
        Expr::Which { pairs } => Expr::Which {
            pairs: pairs
                .iter()
                .map(|(c, v)| (substitute_in_expr(c, subs), substitute_in_expr(v, subs)))
                .collect(),
        },
        Expr::Switch { expr: inner, cases } => Expr::Switch {
            expr: Box::new(substitute_in_expr(inner, subs)),
            cases: cases
                .iter()
                .map(|(p, b)| (substitute_in_expr(p, subs), substitute_in_expr(b, subs)))
                .collect(),
        },
        Expr::Match {
            expr: inner,
            branches,
        } => Expr::Match {
            expr: Box::new(substitute_in_expr(inner, subs)),
            branches: branches
                .iter()
                .map(|b| crate::ast::MatchBranch {
                    pattern: substitute_in_expr(&b.pattern, subs),
                    result: substitute_in_expr(&b.result, subs),
                })
                .collect(),
        },
        Expr::For {
            init,
            condition,
            step,
            body,
        } => Expr::For {
            init: Box::new(substitute_in_expr(init, subs)),
            condition: Box::new(substitute_in_expr(condition, subs)),
            step: Box::new(substitute_in_expr(step, subs)),
            body: Box::new(substitute_in_expr(body, subs)),
        },
        Expr::While { condition, body } => Expr::While {
            condition: Box::new(substitute_in_expr(condition, subs)),
            body: Box::new(substitute_in_expr(body, subs)),
        },
        Expr::Do { body, iterator } => Expr::Do {
            body: Box::new(substitute_in_expr(body, subs)),
            iterator: iterator.clone(),
        },
        Expr::FuncDef {
            name,
            params,
            body,
            delayed,
            guard,
        } => Expr::FuncDef {
            name: name.clone(),
            params: params.clone(),
            body: Box::new(substitute_in_expr(body, subs)),
            delayed: *delayed,
            guard: guard
                .as_ref()
                .map(|g| Box::new(substitute_in_expr(g, subs))),
        },
        Expr::Assign { lhs, rhs } => Expr::Assign {
            lhs: Box::new(substitute_in_expr(lhs, subs)),
            rhs: Box::new(substitute_in_expr(rhs, subs)),
        },
        Expr::DestructAssign { patterns, rhs } => Expr::DestructAssign {
            patterns: patterns
                .iter()
                .map(|p| substitute_in_expr(p, subs))
                .collect(),
            rhs: Box::new(substitute_in_expr(rhs, subs)),
        },
        Expr::PostIncrement { expr: inner } => Expr::PostIncrement {
            expr: Box::new(substitute_in_expr(inner, subs)),
        },
        Expr::PostDecrement { expr: inner } => Expr::PostDecrement {
            expr: Box::new(substitute_in_expr(inner, subs)),
        },
        Expr::Unset { expr: inner } => Expr::Unset {
            expr: Box::new(substitute_in_expr(inner, subs)),
        },
        Expr::ModuleDef {
            name,
            exports,
            body,
        } => Expr::ModuleDef {
            name: name.clone(),
            exports: exports.clone(),
            body: body.iter().map(|b| substitute_in_expr(b, subs)).collect(),
        },
        Expr::Import {
            module,
            selective,
            alias,
        } => Expr::Import {
            module: module.clone(),
            selective: selective.clone(),
            alias: alias.clone(),
        },
        Expr::Export(exports) => Expr::Export(exports.clone()),
        Expr::ClassDef {
            name,
            parent,
            mixins,
            members,
        } => Expr::ClassDef {
            name: name.clone(),
            parent: parent.clone(),
            mixins: mixins.clone(),
            members: members.clone(),
        },
        Expr::Hold(inner) => Expr::Hold(Box::new(substitute_in_expr(inner, subs))),
        Expr::HoldComplete(inner) => Expr::HoldComplete(Box::new(substitute_in_expr(inner, subs))),
        Expr::ReleaseHold(inner) => Expr::ReleaseHold(Box::new(substitute_in_expr(inner, subs))),
        Expr::Information(inner) => Expr::Information(Box::new(substitute_in_expr(inner, subs))),
        other => other.clone(),
    }
}
