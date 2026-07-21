use crate::simplicity_fmt::core::Context;
use either::Either;
use pretty::RcDoc;
use simplicityhl::parse::{
    Assignment, Call, CallName, Expression, ExpressionInner, Function, FunctionParam, Item, Match, MatchArm,
    MatchPattern, Module, Program, SingleExpression, SingleExpressionInner, Statement, TypeAlias, UseDecl, UseItems,
    Visibility,
};
use simplicityhl::pattern::Pattern;
use simplicityhl::types::{AliasedType, TypeDeconstructible};

pub trait Doc {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>>;
}

impl Doc for Program {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let items: Vec<_> = self.items().iter().collect();
        let first_start = items
            .first()
            .and_then(|item| item.span())
            .map(|span| span.start)
            .unwrap_or(context.source.len());

        let mut doc = RcDoc::text(context.source.get(..context.prefix_end)?.to_owned());
        doc = doc.append(gap_doc(context, context.prefix_end, first_start, false));
        let mut previous_end = first_start;

        for (index, item) in items.iter().enumerate() {
            let span = item.span()?;
            if span.end <= span.start {
                return None;
            }
            if index > 0 {
                doc = doc.append(gap_doc(context, previous_end, span.start, true));
            }
            doc = doc.append(item.to_doc(context)?);
            previous_end = span.end;
        }

        doc = doc.append(gap_doc(context, previous_end, context.source.len(), false));
        Some(doc)
    }
}

impl Doc for Item {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let span = self.span()?;
        if context.trivia.has_comment_in(span.start, span.end) {
            context.trivia.take_gap(span.start, span.end);
            let source = match self {
                Item::Function(function) => format_commented_function(function, context)?,
                _ => context.source.get(span.start..span.end)?.to_owned(),
            };
            return Some(RcDoc::text(source));
        }

        match self {
            Item::Function(f) => f.to_doc(context),
            Item::TypeAlias(t) => t.to_doc(context),
            Item::Use(u) => u.to_doc(context),
            Item::Module(m) => m.to_doc(context),
            Item::Ignored => None,
        }
    }
}

#[derive(Clone)]
struct SourceEdit {
    start: usize,
    end: usize,
    replacement: String,
}

fn format_commented_function(function: &Function, context: &Context<'_>) -> Option<String> {
    let span = function.span();
    let mut edits = Vec::new();
    collect_custom_edits(
        function.body(),
        context.source,
        context.config.indent_width.max(0) as usize,
        0,
        &mut edits,
    );

    let mut source = context.source.get(span.start..span.end)?.to_owned();
    edits.sort_by(|left, right| right.start.cmp(&left.start).then_with(|| right.end.cmp(&left.end)));

    for edit in edits {
        let start = edit.start.checked_sub(span.start)?;
        let end = edit.end.checked_sub(span.start)?;
        source.replace_range(start..end, &edit.replacement);
    }

    Some(source)
}

fn collect_custom_edits(
    expression: &Expression,
    source: &str,
    indent_width: usize,
    inherited_indent: usize,
    edits: &mut Vec<SourceEdit>,
) {
    match expression.inner() {
        ExpressionInner::Block(statements, trailing_expression) => {
            for statement in statements.iter() {
                match statement {
                    Statement::Assignment(assignment) => {
                        collect_custom_edits(assignment.expression(), source, indent_width, inherited_indent, edits);
                    }
                    Statement::Expression(expression) => {
                        collect_custom_edits(expression, source, indent_width, inherited_indent, edits);
                    }
                }
            }
            if let Some(expression) = trailing_expression {
                collect_custom_edits(expression, source, indent_width, inherited_indent, edits);
            }
        }
        ExpressionInner::Single(single) => match single.inner() {
            SingleExpressionInner::Either(Either::Left(expression))
            | SingleExpressionInner::Either(Either::Right(expression))
            | SingleExpressionInner::Option(Some(expression))
            | SingleExpressionInner::Expression(expression) => {
                collect_custom_edits(expression, source, indent_width, inherited_indent, edits);
            }
            SingleExpressionInner::Match(match_expression) => {
                collect_match_edits(match_expression, source, indent_width, inherited_indent, edits);
            }
            SingleExpressionInner::Tuple(expressions)
            | SingleExpressionInner::Array(expressions)
            | SingleExpressionInner::List(expressions) => {
                for expression in expressions.iter() {
                    collect_custom_edits(expression, source, indent_width, inherited_indent, edits);
                }
            }
            SingleExpressionInner::Call(call) => {
                for expression in call.args() {
                    collect_custom_edits(expression, source, indent_width, inherited_indent, edits);
                }
            }
            SingleExpressionInner::Option(None)
            | SingleExpressionInner::Boolean(..)
            | SingleExpressionInner::Decimal(..)
            | SingleExpressionInner::Binary(..)
            | SingleExpressionInner::Hexadecimal(..)
            | SingleExpressionInner::Witness(..)
            | SingleExpressionInner::Parameter(..)
            | SingleExpressionInner::Variable(..) => {}
        },
    }
}

fn collect_match_edits(
    match_expression: &Match,
    source: &str,
    indent_width: usize,
    inherited_indent: usize,
    edits: &mut Vec<SourceEdit>,
) {
    for arm in [match_expression.left(), match_expression.right()] {
        let expression = arm.expression();
        let span = expression.span();
        let is_block = source.get(span.start..).is_some_and(|source| source.starts_with('{'));

        if !is_block {
            if is_empty_tuple(expression) {
                edits.push(SourceEdit {
                    start: span.start,
                    end: span.end,
                    replacement: "{}".to_owned(),
                });
            } else {
                // TODO: calculate efficiently ident
                let arm_indent = indentation_at(source, span.start);
                let effective_arm_indent = format!("{arm_indent}{}", " ".repeat(inherited_indent));
                let body_indent = format!("{effective_arm_indent}{}", " ".repeat(indent_width));
                indent_source_lines(source, span.start, span.end, indent_width, edits);
                edits.push(SourceEdit {
                    start: span.start,
                    end: span.start,
                    replacement: format!("{{\n{body_indent}"),
                });
                edits.push(SourceEdit {
                    start: span.end,
                    end: span.end,
                    replacement: format!("\n{effective_arm_indent}}}"),
                });
            }
        }

        let nested_indent = if !is_block && !is_empty_tuple(expression) {
            inherited_indent.saturating_add(indent_width)
        } else {
            inherited_indent
        };
        collect_custom_edits(expression, source, indent_width, nested_indent, edits);
    }
}

fn indent_source_lines(source: &str, start: usize, end: usize, indent_width: usize, edits: &mut Vec<SourceEdit>) {
    let Some(expression) = source.get(start..end) else {
        return;
    };

    for (offset, _) in expression.match_indices('\n') {
        let line_start = start + offset + 1;
        if line_start < end {
            edits.push(SourceEdit {
                start: line_start,
                end: line_start,
                replacement: " ".repeat(indent_width),
            });
        }
    }
}

fn indentation_at(source: &str, offset: usize) -> &str {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..offset];
    let indent_width = line
        .find(|character: char| !character.is_whitespace())
        .unwrap_or(line.len());
    &source[line_start..line_start + indent_width]
}

fn is_empty_tuple(expression: &Expression) -> bool {
    matches!(
        expression.inner(),
        ExpressionInner::Single(single)
            if matches!(single.inner(), SingleExpressionInner::Tuple(values) if values.is_empty())
    )
}

fn gap_doc<'src>(context: &mut Context<'_>, start: usize, end: usize, between_items: bool) -> RcDoc<'src> {
    if start > end || end > context.source.len() {
        return RcDoc::nil();
    }

    let trivia = context.trivia.take_gap(start, end);
    if trivia.iter().any(|trivia| trivia.is_comment()) {
        return RcDoc::text(context.source[start..end].to_owned());
    }

    let newline_count = trivia.iter().filter(|trivia| trivia.is_newline()).count();

    if start == end {
        RcDoc::nil()
    } else if between_items && newline_count >= 2 {
        RcDoc::hardline().append(RcDoc::hardline())
    } else {
        RcDoc::hardline()
    }
}

impl Doc for Visibility {
    fn to_doc<'src>(&self, _context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        match self {
            Visibility::Public => Some(RcDoc::text("pub ")),
            Visibility::Private => Some(RcDoc::nil()),
        }
    }
}

impl Doc for Function {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let vis = self.visibility().to_doc(context)?;
        let name = RcDoc::as_string(self.name());

        let params: Vec<_> = self.params().iter().filter_map(|p| p.to_doc(context)).collect();
        let params_doc = if params.is_empty() {
            RcDoc::text("()")
        } else {
            RcDoc::text("(")
                .append(RcDoc::line_())
                .append(RcDoc::intersperse(params, RcDoc::text(",").append(RcDoc::line())))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::line_())
                .append(RcDoc::text(")"))
                .group()
        };

        let ret_doc = match self.ret() {
            Some(ty) => RcDoc::text(" -> ").append(RcDoc::as_string(ty)),
            None => RcDoc::nil(),
        };

        let sig = vis
            .append(RcDoc::text("fn "))
            .append(name)
            .append(params_doc)
            .append(ret_doc);

        let body = self.body().to_doc(context)?;

        Some(sig.append(RcDoc::space()).append(body))
    }
}

impl Doc for FunctionParam {
    fn to_doc<'src>(&self, _context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        Some(
            RcDoc::as_string(self.identifier())
                .append(RcDoc::text(": "))
                .append(RcDoc::as_string(self.ty())),
        )
    }
}

impl Doc for UseDecl {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let vis = self.visibility().to_doc(context)?;
        let mut path_str = String::new();
        for (i, p) in self.path().iter().enumerate() {
            if i > 0 {
                path_str.push_str("::");
            }
            path_str.push_str(p.as_inner());
        }
        if !self.path().is_empty() {
            path_str.push_str("::");
        }

        let items_doc = match self.items() {
            UseItems::Single((ident, alias)) => {
                let mut s = ident.as_inner().to_string();
                if let Some(a) = alias {
                    s.push_str(" as ");
                    s.push_str(a.as_inner());
                }
                RcDoc::text(s)
            }
            UseItems::List(items) => {
                let docs: Vec<_> = items
                    .iter()
                    .map(|(ident, alias)| {
                        let mut s = ident.as_inner().to_string();
                        if let Some(a) = alias {
                            s.push_str(" as ");
                            s.push_str(a.as_inner());
                        }
                        RcDoc::text(s)
                    })
                    .collect();
                RcDoc::text("{")
                    .append(RcDoc::line_())
                    .append(RcDoc::intersperse(docs, RcDoc::text(",").append(RcDoc::line())))
                    .nest(context.config.indent_width as isize)
                    .append(RcDoc::line_())
                    .append(RcDoc::text("}"))
                    .group()
            }
        };

        Some(
            vis.append(RcDoc::text("use "))
                .append(RcDoc::text(path_str))
                .append(items_doc)
                .append(RcDoc::text(";")),
        )
    }
}

impl Doc for TypeAlias {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let vis = self.visibility().to_doc(context)?;
        Some(
            vis.append(RcDoc::text("type "))
                .append(RcDoc::as_string(self.name()))
                .append(RcDoc::text(" = "))
                .append(RcDoc::as_string(self.ty()))
                .append(RcDoc::text(";")),
        )
    }
}

impl Doc for Module {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let vis = self.visibility().to_doc(context)?;

        let items: Vec<_> = self.items().iter().filter_map(|i| i.to_doc(context)).collect();
        let body = if items.is_empty() {
            RcDoc::text("{}")
        } else {
            RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(RcDoc::intersperse(items, RcDoc::hardline().append(RcDoc::hardline())))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::hardline())
                .append(RcDoc::text("}"))
        };

        Some(
            vis.append(RcDoc::text("mod "))
                .append(RcDoc::as_string(self.name()))
                .append(RcDoc::space())
                .append(body),
        )
    }
}

impl Doc for Expression {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        match self.inner() {
            ExpressionInner::Single(s) => s.to_doc(context),
            ExpressionInner::Block(stmts, expr) => {
                let stmts_doc: Vec<_> = stmts.iter().filter_map(|s| s.to_doc(context)).collect();
                let expr_doc = expr.as_ref().and_then(|e| e.to_doc(context));

                let mut inner = RcDoc::nil();
                let has_stmts = !stmts_doc.is_empty();

                if has_stmts {
                    inner = inner.append(RcDoc::intersperse(stmts_doc, RcDoc::hardline()));
                }

                if let Some(e) = expr_doc {
                    if has_stmts {
                        inner = inner.append(RcDoc::hardline());
                    }
                    inner = inner.append(e);
                }

                if has_stmts || expr.is_some() {
                    Some(
                        RcDoc::text("{")
                            .append(RcDoc::hardline())
                            .append(inner)
                            .nest(context.config.indent_width as isize)
                            .append(RcDoc::hardline())
                            .append(RcDoc::text("}")),
                    )
                } else {
                    Some(RcDoc::text("{}"))
                }
            }
        }
    }
}

impl Doc for Statement {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        match self {
            Statement::Assignment(a) => a.to_doc(context),
            Statement::Expression(e) => Some(e.to_doc(context)?.append(RcDoc::text(";"))),
        }
    }
}

impl Doc for Assignment {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        Some(
            RcDoc::text("let ")
                .append(self.pattern().to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(self.ty().to_doc(context)?)
                .append(RcDoc::text(" = "))
                .append(self.expression().to_doc(context)?)
                .append(RcDoc::text(";")),
        )
    }
}

impl Doc for AliasedType {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        if let Some(alias) = self.as_alias() {
            return Some(RcDoc::as_string(alias));
        }
        if let Some(alias) = self.as_builtin() {
            return Some(RcDoc::as_string(alias));
        }
        if self.is_boolean() {
            return Some(RcDoc::text("bool"));
        }
        if let Some(integer) = self.as_integer() {
            return Some(RcDoc::as_string(integer));
        }
        if let Some((left, right)) = self.as_either() {
            return Some(
                RcDoc::text("Either<")
                    .append(RcDoc::line_())
                    .append(left.to_doc(context)?)
                    .append(RcDoc::text(",").append(RcDoc::line()))
                    .append(right.to_doc(context)?)
                    .nest(context.config.indent_width as isize)
                    .append(RcDoc::line_())
                    .append(RcDoc::text(">"))
                    .group(),
            );
        }
        if let Some(inner) = self.as_option() {
            return Some(
                RcDoc::text("Option<")
                    .append(RcDoc::line_())
                    .append(inner.to_doc(context)?)
                    .nest(context.config.indent_width as isize)
                    .append(RcDoc::line_())
                    .append(RcDoc::text(">"))
                    .group(),
            );
        }
        if let Some(elements) = self.as_tuple() {
            if elements.is_empty() {
                return Some(RcDoc::text("()"));
            }

            let docs: Option<Vec<_>> = elements.iter().map(|element| element.to_doc(context)).collect();
            let mut elements_doc = RcDoc::intersperse(docs?, RcDoc::text(",").append(RcDoc::line()));
            if elements.len() == 1 {
                elements_doc = elements_doc.append(RcDoc::text(","));
            }

            return Some(
                RcDoc::text("(")
                    .append(RcDoc::line_())
                    .append(elements_doc.nest(context.config.indent_width as isize))
                    .append(RcDoc::line_())
                    .append(RcDoc::text(")"))
                    .group(),
            );
        }
        if let Some((element, size)) = self.as_array() {
            return Some(
                RcDoc::text("[")
                    .append(RcDoc::line_())
                    .append(element.to_doc(context)?)
                    .append(RcDoc::text("; "))
                    .append(RcDoc::as_string(size))
                    .nest(context.config.indent_width as isize)
                    .append(RcDoc::line_())
                    .append(RcDoc::text("]"))
                    .group(),
            );
        }
        if let Some((element, bound)) = self.as_list() {
            return Some(
                RcDoc::text("List<")
                    .append(RcDoc::line_())
                    .append(element.to_doc(context)?)
                    .append(RcDoc::text(", "))
                    .append(RcDoc::as_string(bound))
                    .nest(context.config.indent_width as isize)
                    .append(RcDoc::line_())
                    .append(RcDoc::text(">"))
                    .group(),
            );
        }

        None
    }
}

impl Doc for Pattern {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        match self {
            Pattern::Ignore => Some(RcDoc::text("_")),
            Pattern::Identifier(id) => Some(RcDoc::as_string(id)),
            Pattern::Tuple(patterns) => {
                let docs: Vec<_> = patterns.iter().filter_map(|p| p.to_doc(context)).collect();
                Some(
                    RcDoc::text("(")
                        .append(RcDoc::intersperse(docs, RcDoc::text(", ")))
                        .append(RcDoc::text(")")),
                )
            }
            Pattern::Array(patterns) => {
                let docs: Vec<_> = patterns.iter().filter_map(|p| p.to_doc(context)).collect();
                Some(
                    RcDoc::text("[")
                        .append(RcDoc::intersperse(docs, RcDoc::text(", ")))
                        .append(RcDoc::text("]")),
                )
            }
        }
    }
}

impl Doc for SingleExpression {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        match self.inner() {
            SingleExpressionInner::Either(Either::Left(e)) => Some(
                RcDoc::text("Left(")
                    .append(e.to_doc(context)?)
                    .append(RcDoc::text(")"))
                    .group(),
            ),
            SingleExpressionInner::Either(Either::Right(e)) => Some(
                RcDoc::text("Right(")
                    .append(e.to_doc(context)?)
                    .append(RcDoc::text(")"))
                    .group(),
            ),
            SingleExpressionInner::Option(Some(e)) => Some(
                RcDoc::text("Some(")
                    .append(e.to_doc(context)?)
                    .append(RcDoc::text(")"))
                    .group(),
            ),
            SingleExpressionInner::Option(None) => Some(RcDoc::text("None")),
            SingleExpressionInner::Boolean(b) => Some(RcDoc::as_string(b)),
            SingleExpressionInner::Decimal(d) => Some(RcDoc::as_string(d)),
            SingleExpressionInner::Binary(b) => Some(RcDoc::text("0b").append(RcDoc::as_string(b))),
            SingleExpressionInner::Hexadecimal(h) => Some(RcDoc::text("0x").append(RcDoc::as_string(h))),
            SingleExpressionInner::Witness(w) => Some(RcDoc::text("witness::").append(RcDoc::as_string(w))),
            SingleExpressionInner::Parameter(p) => Some(RcDoc::text("param::").append(RcDoc::as_string(p))),
            SingleExpressionInner::Variable(v) => Some(RcDoc::as_string(v)),
            SingleExpressionInner::Call(c) => c.to_doc(context),
            SingleExpressionInner::Expression(e) => Some(
                RcDoc::text("(")
                    .append(e.to_doc(context)?)
                    .append(RcDoc::text(")"))
                    .group(),
            ),
            SingleExpressionInner::Match(m) => m.to_doc(context),
            SingleExpressionInner::Tuple(exprs) => {
                let docs: Vec<_> = exprs.iter().filter_map(|e| e.to_doc(context)).collect();
                Some(
                    RcDoc::text("(")
                        .append(RcDoc::line_())
                        .append(RcDoc::intersperse(docs, RcDoc::text(",").append(RcDoc::line())))
                        .nest(context.config.indent_width as isize)
                        .append(RcDoc::line_())
                        .append(RcDoc::text(")"))
                        .group(),
                )
            }
            SingleExpressionInner::Array(exprs) => {
                let docs: Vec<_> = exprs.iter().filter_map(|e| e.to_doc(context)).collect();
                Some(
                    RcDoc::text("[")
                        .append(RcDoc::line_())
                        .append(RcDoc::intersperse(docs, RcDoc::text(",").append(RcDoc::line())))
                        .nest(context.config.indent_width as isize)
                        .append(RcDoc::line_())
                        .append(RcDoc::text("]"))
                        .group(),
                )
            }
            SingleExpressionInner::List(exprs) => {
                let docs: Vec<_> = exprs.iter().filter_map(|e| e.to_doc(context)).collect();
                Some(
                    RcDoc::text("list![")
                        .append(RcDoc::line_())
                        .append(RcDoc::intersperse(docs, RcDoc::text(",").append(RcDoc::line())))
                        .nest(context.config.indent_width as isize)
                        .append(RcDoc::line_())
                        .append(RcDoc::text("]"))
                        .group(),
                )
            }
        }
    }
}

impl Doc for Call {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let name_doc = RcDoc::as_string(self.name());
        let args: Vec<_> = self.args().iter().filter_map(|a| a.to_doc(context)).collect();

        let args_doc = if args.is_empty() {
            RcDoc::text("()")
        } else {
            RcDoc::text("(")
                .append(RcDoc::line_())
                .append(RcDoc::intersperse(args, RcDoc::text(",").append(RcDoc::line())))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::line_())
                .append(RcDoc::text(")"))
                .group()
        };

        Some(name_doc.append(args_doc))
    }
}

impl Doc for Match {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let scrutinee = self.scrutinee().to_doc(context)?;

        let left_arm = self.left().to_doc(context)?;
        let right_arm = self.right().to_doc(context)?;

        let body = RcDoc::text("{")
            .append(RcDoc::hardline())
            .append(left_arm.append(RcDoc::text(",")))
            .append(RcDoc::hardline())
            .append(right_arm)
            .nest(context.config.indent_width as isize)
            .append(RcDoc::hardline())
            .append(RcDoc::text("}"));

        Some(
            RcDoc::text("match ")
                .append(scrutinee)
                .append(RcDoc::space())
                .append(body),
        )
    }
}

impl Doc for MatchArm {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let pat_doc = match self.pattern() {
            MatchPattern::Left(p, ty) => RcDoc::text("Left(")
                .append(p.to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(RcDoc::as_string(ty))
                .append(RcDoc::text(")")),
            MatchPattern::Right(p, ty) => RcDoc::text("Right(")
                .append(p.to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(RcDoc::as_string(ty))
                .append(RcDoc::text(")")),
            MatchPattern::None => RcDoc::text("None"),
            MatchPattern::Some(p, ty) => RcDoc::text("Some(")
                .append(p.to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(RcDoc::as_string(ty))
                .append(RcDoc::text(")")),
            MatchPattern::False => RcDoc::text("false"),
            MatchPattern::True => RcDoc::text("true"),
        };

        let expr_doc = self.expression().to_doc(context)?;
        let body = match self.expression().inner() {
            ExpressionInner::Block(..) => expr_doc,
            ExpressionInner::Single(single) if matches!(single.inner(), SingleExpressionInner::Tuple(values) if values.is_empty()) => {
                RcDoc::text("{}")
            }
            ExpressionInner::Single(..) => RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(expr_doc)
                .nest(context.config.indent_width as isize)
                .append(RcDoc::hardline())
                .append(RcDoc::text("}")),
        };

        Some(pat_doc.append(RcDoc::text(" => ")).append(body))
    }
}
