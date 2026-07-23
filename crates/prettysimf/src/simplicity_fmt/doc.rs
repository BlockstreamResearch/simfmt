use crate::simplicity_fmt::core::Context;
use either::Either;
use pretty::RcDoc;
use simplicityhl::parse::{
    Assignment, Call, EnumConstruction, EnumDeclaration, EnumMatch, EnumMatchArm, EnumVariant, Expression,
    ExpressionInner, Function, FunctionParam, Item, Match, MatchArm, MatchPattern, Module, Program, SingleExpression,
    SingleExpressionInner, Statement, TypeAlias, UseDecl, UseItems, Visibility,
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
            Item::EnumDeclaration(declaration) => declaration.to_doc(context),
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

#[derive(Clone, Copy)]
struct LineIndent {
    start: usize,
    end: usize,
}

struct CustomEditContext<'src> {
    source: &'src str,
    line_indents: Vec<LineIndent>,
    indent: String,
}

impl<'src> CustomEditContext<'src> {
    fn new(source: &'src str, indent_width: usize) -> Self {
        let mut line_indents = Vec::new();
        let mut line_start = 0;

        loop {
            let line_end = source[line_start..]
                .find('\n')
                .map_or(source.len(), |offset| line_start + offset);
            let indent_end = source[line_start..line_end]
                .find(|character: char| !character.is_whitespace())
                .map_or(line_end, |offset| line_start + offset);

            line_indents.push(LineIndent {
                start: line_start,
                end: indent_end,
            });

            if line_end == source.len() {
                break;
            }
            line_start = line_end + 1;
        }

        Self {
            source,
            line_indents,
            indent: " ".repeat(indent_width),
        }
    }

    fn indentation_at(&self, offset: usize) -> &'src str {
        let index = match self.line_indents.binary_search_by_key(&offset, |line| line.start) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line = self.line_indents[index];
        &self.source[line.start..line.end]
    }

    fn indent_from(&self, base: &str, inherited_depth: usize) -> String {
        let mut indentation = String::with_capacity(base.len() + self.indent.len() * inherited_depth);
        indentation.push_str(base);
        for _ in 0..inherited_depth {
            indentation.push_str(&self.indent);
        }
        indentation
    }
}

fn format_commented_function(function: &Function, context: &Context<'_>) -> Option<String> {
    let span = function.span();
    let mut edits = Vec::new();
    let edit_context = CustomEditContext::new(context.source, context.config.indent_width.max(0));
    collect_custom_edits(function.body(), &edit_context, 0, &mut edits);

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
    context: &CustomEditContext<'_>,
    inherited_indent_depth: usize,
    edits: &mut Vec<SourceEdit>,
) {
    match expression.inner() {
        ExpressionInner::Block(statements, trailing_expression) => {
            for statement in statements.iter() {
                match statement {
                    Statement::Assignment(assignment) => {
                        collect_custom_edits(assignment.expression(), context, inherited_indent_depth, edits);
                    }
                    Statement::Expression(expression) => {
                        collect_custom_edits(expression, context, inherited_indent_depth, edits);
                    }
                }
            }
            if let Some(expression) = trailing_expression {
                collect_custom_edits(expression, context, inherited_indent_depth, edits);
            }
        }
        ExpressionInner::Single(single) => match single.inner() {
            SingleExpressionInner::Either(Either::Left(expression))
            | SingleExpressionInner::Either(Either::Right(expression))
            | SingleExpressionInner::Option(Some(expression))
            | SingleExpressionInner::Expression(expression) => {
                collect_custom_edits(expression, context, inherited_indent_depth, edits);
            }
            SingleExpressionInner::Match(match_expression) => {
                collect_custom_edits(match_expression.scrutinee(), context, inherited_indent_depth, edits);
                collect_match_edits(match_expression, context, inherited_indent_depth, edits);
            }
            SingleExpressionInner::EnumMatch(match_expression) => {
                collect_custom_edits(match_expression.scrutinee(), context, inherited_indent_depth, edits);
                collect_match_expression_edits(
                    match_expression.arms().iter().map(EnumMatchArm::expression),
                    context,
                    inherited_indent_depth,
                    edits,
                );
            }
            SingleExpressionInner::EnumConstruction(construction) => {
                for expression in construction.args() {
                    collect_custom_edits(expression, context, inherited_indent_depth, edits);
                }
            }
            SingleExpressionInner::Tuple(expressions)
            | SingleExpressionInner::Array(expressions)
            | SingleExpressionInner::List(expressions) => {
                for expression in expressions.iter() {
                    collect_custom_edits(expression, context, inherited_indent_depth, edits);
                }
            }
            SingleExpressionInner::Call(call) => {
                for expression in call.args() {
                    collect_custom_edits(expression, context, inherited_indent_depth, edits);
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
    context: &CustomEditContext<'_>,
    inherited_indent_depth: usize,
    edits: &mut Vec<SourceEdit>,
) {
    collect_match_expression_edits(
        [match_expression.left(), match_expression.right()]
            .into_iter()
            .map(MatchArm::expression),
        context,
        inherited_indent_depth,
        edits,
    );
}

fn collect_match_expression_edits<'src>(
    expressions: impl IntoIterator<Item = &'src Expression>,
    context: &CustomEditContext<'_>,
    inherited_indent_depth: usize,
    edits: &mut Vec<SourceEdit>,
) {
    for expression in expressions {
        let span = expression.span();
        let is_block = context
            .source
            .get(span.start..)
            .is_some_and(|source| source.starts_with('{'));

        if !is_block {
            if is_empty_tuple(expression) {
                edits.push(SourceEdit {
                    start: span.start,
                    end: span.end,
                    replacement: "{}".to_owned(),
                });
            } else {
                let arm_indent = context.indentation_at(span.start);
                let effective_arm_indent = context.indent_from(arm_indent, inherited_indent_depth);
                let body_indent = context.indent_from(&effective_arm_indent, 1);
                indent_source_lines(context, span.start, span.end, edits);
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
            inherited_indent_depth.saturating_add(1)
        } else {
            inherited_indent_depth
        };
        collect_custom_edits(expression, context, nested_indent, edits);
    }
}

fn indent_source_lines(context: &CustomEditContext<'_>, start: usize, end: usize, edits: &mut Vec<SourceEdit>) {
    let Some(expression) = context.source.get(start..end) else {
        return;
    };

    for (offset, _) in expression.match_indices('\n') {
        let line_start = start + offset + 1;
        if line_start < end {
            edits.push(SourceEdit {
                start: line_start,
                end: line_start,
                replacement: context.indent.clone(),
            });
        }
    }
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

    if between_items {
        if newline_count >= 2 {
            RcDoc::hardline().append(RcDoc::hardline())
        } else {
            RcDoc::hardline()
        }
    } else if start == end {
        RcDoc::nil()
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

impl Doc for EnumDeclaration {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let vis = self.visibility().to_doc(context)?;
        let variants: Option<Vec<_>> = self.variants().iter().map(|variant| variant.to_doc(context)).collect();
        let variants = variants?;

        let body = if variants.is_empty() {
            RcDoc::text("{}")
        } else {
            RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(RcDoc::intersperse(variants, RcDoc::text(",").append(RcDoc::hardline())))
                .append(RcDoc::text(","))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::hardline())
                .append(RcDoc::text("}"))
        };

        Some(
            vis.append(RcDoc::text("enum "))
                .append(RcDoc::as_string(self.name()))
                .append(RcDoc::space())
                .append(body),
        )
    }
}

impl Doc for EnumVariant {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let name = RcDoc::as_string(self.name());
        if self.payload().is_empty() {
            return Some(name);
        }

        let payload: Option<Vec<_>> = self.payload().iter().map(|ty| ty.to_doc(context)).collect();
        Some(
            name.append(RcDoc::text("("))
                .append(RcDoc::line_())
                .append(RcDoc::intersperse(payload?, RcDoc::text(",").append(RcDoc::line())))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::line_())
                .append(RcDoc::text(")"))
                .group(),
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
                let mut inner = RcDoc::nil();
                let mut previous_statement: Option<&Statement> = None;

                for statement in stmts.iter() {
                    if let Some(previous) = previous_statement {
                        let previous_end =
                            context.semicolon_end_between(previous.span().end, statement.span().start)?;
                        inner = inner.append(gap_doc(context, previous_end, statement.span().start, true));
                    }

                    inner = inner.append(statement.to_doc(context)?);
                    previous_statement = Some(statement);
                }

                if let Some(expression) = expr {
                    if let Some(previous) = previous_statement {
                        let previous_end =
                            context.semicolon_end_between(previous.span().end, expression.span().start)?;
                        inner = inner.append(gap_doc(context, previous_end, expression.span().start, true));
                    }
                    inner = inner.append(expression.to_doc(context)?);
                }

                if !stmts.is_empty() || expr.is_some() {
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
                .append(RcDoc::text(";"))
                .group(),
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
                if patterns.is_empty() {
                    return Some(RcDoc::text("()"));
                }

                let docs: Option<Vec<_>> = patterns.iter().map(|pattern| pattern.to_doc(context)).collect();
                Some(
                    RcDoc::text("(")
                        .append(
                            RcDoc::line_()
                                .append(RcDoc::intersperse(docs?, RcDoc::text(",").append(RcDoc::line())))
                                .nest(context.config.indent_width as isize),
                        )
                        .append(RcDoc::line_())
                        .append(RcDoc::text(")")),
                )
            }
            Pattern::Array(patterns) => {
                if patterns.is_empty() {
                    return Some(RcDoc::text("[]"));
                }

                let docs: Option<Vec<_>> = patterns.iter().map(|pattern| pattern.to_doc(context)).collect();
                Some(
                    RcDoc::text("[")
                        .append(
                            RcDoc::line_()
                                .append(RcDoc::intersperse(docs?, RcDoc::text(",").append(RcDoc::line())))
                                .nest(context.config.indent_width as isize),
                        )
                        .append(RcDoc::line_())
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
            SingleExpressionInner::EnumMatch(m) => m.to_doc(context),
            SingleExpressionInner::EnumConstruction(construction) => construction.to_doc(context),
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

impl Doc for EnumConstruction {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let head = RcDoc::text(self.enum_path_string())
            .append(RcDoc::text("::"))
            .append(RcDoc::as_string(self.variant()));
        if self.args().is_empty() {
            return Some(head);
        }

        let args: Option<Vec<_>> = self.args().iter().map(|argument| argument.to_doc(context)).collect();
        Some(
            head.append(RcDoc::text("("))
                .append(RcDoc::line_())
                .append(RcDoc::intersperse(args?, RcDoc::text(",").append(RcDoc::line())))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::line_())
                .append(RcDoc::text(")"))
                .group(),
        )
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

impl Doc for EnumMatch {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let scrutinee = self.scrutinee().to_doc(context)?;
        let arms: Option<Vec<_>> = self.arms().iter().map(|arm| arm.to_doc(context)).collect();
        let arms = arms?;

        let body = if arms.is_empty() {
            RcDoc::text("{}")
        } else {
            RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(RcDoc::intersperse(arms, RcDoc::text(",").append(RcDoc::hardline())))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::hardline())
                .append(RcDoc::text("}"))
        };

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
                .append(RcDoc::text(")"))
                .group(),
            MatchPattern::Right(p, ty) => RcDoc::text("Right(")
                .append(p.to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(RcDoc::as_string(ty))
                .append(RcDoc::text(")"))
                .group(),
            MatchPattern::None => RcDoc::text("None"),
            MatchPattern::Some(p, ty) => RcDoc::text("Some(")
                .append(p.to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(RcDoc::as_string(ty))
                .append(RcDoc::text(")"))
                .group(),
            MatchPattern::False => RcDoc::text("false"),
            MatchPattern::True => RcDoc::text("true"),
        };

        Some(
            pat_doc
                .append(RcDoc::text(" => "))
                .append(match_arm_body(self.expression(), context)?),
        )
    }
}

impl Doc for EnumMatchArm {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let mut head = RcDoc::text(self.enum_path_string())
            .append(RcDoc::text("::"))
            .append(RcDoc::as_string(self.variant()));

        if !self.bindings().is_empty() {
            let bindings: Option<Vec<_>> = self
                .bindings()
                .iter()
                .map(|(pattern, ty)| {
                    Some(
                        pattern
                            .to_doc(context)?
                            .append(RcDoc::text(": "))
                            .append(ty.to_doc(context)?),
                    )
                })
                .collect();
            head = head
                .append(RcDoc::text("("))
                .append(RcDoc::line_())
                .append(RcDoc::intersperse(bindings?, RcDoc::text(",").append(RcDoc::line())))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::line_())
                .append(RcDoc::text(")"))
                .group();
        }

        Some(
            head.append(RcDoc::text(" => "))
                .append(match_arm_body(self.expression(), context)?),
        )
    }
}

fn match_arm_body<'src>(expression: &Expression, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
    let expression_doc = expression.to_doc(context)?;
    match expression.inner() {
        ExpressionInner::Block(..) => Some(expression_doc),
        ExpressionInner::Single(single) if matches!(single.inner(), SingleExpressionInner::Tuple(values) if values.is_empty()) => {
            Some(RcDoc::text("{}"))
        }
        ExpressionInner::Single(..) => Some(
            RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(expression_doc)
                .nest(context.config.indent_width as isize)
                .append(RcDoc::hardline())
                .append(RcDoc::text("}")),
        ),
    }
}
