use crate::simplicity_fmt::core::{Context, TriviaCursor};
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

fn type_doc<'src>(context: &mut Context<'_>, ty: &AliasedType, start: usize, end: usize) -> Option<RcDoc<'src>> {
    context.exec_with_type_range(start, end, |context| ty.to_doc(context))
}

/// Converts source text into a document without embedding line breaks in `RcDoc::text`.
///
/// The `pretty` crate requires text nodes to contain no line breaks. Formatting emits
/// canonical LF line endings; the outer formatting pipeline reapplies the configured
/// newline style before emitting the result.
fn source_doc<'src>(source: &str) -> RcDoc<'src> {
    let mut doc = RcDoc::nil();
    let mut remaining = source;

    while let Some(newline) = remaining.find('\n') {
        let (line, rest) = remaining.split_at(newline);
        let line = line.strip_suffix('\r').unwrap_or(line);
        doc = doc.append(RcDoc::text(line.to_owned())).append(RcDoc::hardline());
        remaining = &rest[1..];
    }

    doc.append(RcDoc::text(remaining.to_owned()))
}

impl Doc for Program {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let items: Vec<_> = self.items().iter().collect();
        let first_start = items
            .first()
            .and_then(|item| item.span())
            .map(|span| span.start)
            .unwrap_or(context.source.len());

        let mut doc = source_doc(context.source.get(..context.prefix_end)?);
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
            return Some(source_doc(&source));
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

struct CustomEditContext<'src> {
    source: &'src str,
    trivia: &'src TriviaCursor,
}

impl<'src> CustomEditContext<'src> {
    fn new(source: &'src str, trivia: &'src TriviaCursor) -> Self {
        Self { source, trivia }
    }
}

fn format_commented_function(function: &Function, context: &Context<'_>) -> Option<String> {
    let span = function.span();
    let mut edits = Vec::new();
    let edit_context = CustomEditContext::new(context.source, &context.trivia);
    collect_custom_edits(function.body(), &edit_context, &mut edits);

    let mut source = context.source.get(span.start..span.end)?.to_owned();
    edits.sort_by(|left, right| right.start.cmp(&left.start).then_with(|| right.end.cmp(&left.end)));

    for edit in edits {
        let start = edit.start.checked_sub(span.start)?;
        let end = edit.end.checked_sub(span.start)?;
        source.replace_range(start..end, &edit.replacement);
    }

    Some(source)
}

fn collect_custom_edits(expression: &Expression, context: &CustomEditContext<'_>, edits: &mut Vec<SourceEdit>) {
    match expression.inner() {
        ExpressionInner::Block(statements, trailing_expression) => {
            for statement in statements.iter() {
                match statement {
                    Statement::Assignment(assignment) => {
                        collect_custom_edits(assignment.expression(), context, edits);
                    }
                    Statement::Expression(expression) => {
                        collect_custom_edits(expression, context, edits);
                    }
                }
            }
            if let Some(expression) = trailing_expression {
                collect_custom_edits(expression, context, edits);
            }
        }
        ExpressionInner::Single(single) => match single.inner() {
            SingleExpressionInner::Either(Either::Left(expression))
            | SingleExpressionInner::Either(Either::Right(expression))
            | SingleExpressionInner::Option(Some(expression))
            | SingleExpressionInner::Expression(expression) => {
                collect_custom_edits(expression, context, edits);
            }
            SingleExpressionInner::Match(match_expression) => {
                collect_custom_edits(match_expression.scrutinee(), context, edits);
                collect_match_edits(match_expression, context, edits);
            }
            SingleExpressionInner::EnumMatch(match_expression) => {
                collect_custom_edits(match_expression.scrutinee(), context, edits);
                collect_match_expression_edits(
                    match_expression.arms().iter().map(EnumMatchArm::expression),
                    context,
                    edits,
                );
            }
            SingleExpressionInner::EnumConstruction(construction) => {
                for expression in construction.args() {
                    collect_custom_edits(expression, context, edits);
                }
            }
            SingleExpressionInner::Tuple(expressions)
            | SingleExpressionInner::Array(expressions)
            | SingleExpressionInner::List(expressions) => {
                for expression in expressions.iter() {
                    collect_custom_edits(expression, context, edits);
                }
            }
            SingleExpressionInner::Call(call) => {
                for expression in call.args() {
                    collect_custom_edits(expression, context, edits);
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

fn collect_match_edits(match_expression: &Match, context: &CustomEditContext<'_>, edits: &mut Vec<SourceEdit>) {
    collect_match_expression_edits(
        [match_expression.left(), match_expression.right()]
            .into_iter()
            .map(MatchArm::expression),
        context,
        edits,
    );
}

fn collect_match_expression_edits<'src>(
    expressions: impl IntoIterator<Item = &'src Expression>,
    context: &CustomEditContext<'_>,
    edits: &mut Vec<SourceEdit>,
) {
    for expression in expressions {
        let outer_span = expression.span();
        let (expression, removed_block_depth) = redundant_match_arm_expression(expression, context);
        let inner_span = expression.span();

        if removed_block_depth > 0 {
            edits.push(SourceEdit {
                start: outer_span.start,
                end: inner_span.start,
                replacement: String::new(),
            });
            edits.push(SourceEdit {
                start: inner_span.end,
                end: outer_span.end,
                replacement: String::new(),
            });
            if !context
                .source
                .get(outer_span.end..)
                .is_some_and(|source| source.trim_start().starts_with(','))
            {
                edits.push(SourceEdit {
                    start: outer_span.end,
                    end: outer_span.end,
                    replacement: ",".to_owned(),
                });
            }
        }

        collect_custom_edits(expression, context, edits);
    }
}

fn redundant_match_arm_expression<'a>(
    mut expression: &'a Expression,
    context: &CustomEditContext<'_>,
) -> (&'a Expression, usize) {
    let original = expression;
    let mut removed_depth = 0;

    while let ExpressionInner::Block(statements, Some(inner)) = expression.inner() {
        let outer_span = expression.span();
        let inner_span = inner.span();
        let clean_prefix = context
            .source
            .get(outer_span.start..inner_span.start)
            .is_some_and(|source| {
                source
                    .chars()
                    .all(|character| character == '{' || character.is_whitespace())
            });
        let clean_suffix = context
            .source
            .get(inner_span.end..outer_span.end)
            .is_some_and(|source| {
                source
                    .chars()
                    .all(|character| character == '}' || character.is_whitespace())
            });

        if !statements.is_empty()
            || !clean_prefix
            || !clean_suffix
            || context
                .trivia
                .has_comment_in(expression.span().start, expression.span().end)
        {
            break;
        }

        expression = inner;
        removed_depth += 1;
    }

    let is_inline = context
        .source
        .get(expression.span().start..expression.span().end)
        .is_some_and(|source| !source.contains('\n'));

    if is_inline {
        (expression, removed_depth)
    } else {
        (original, 0)
    }
}

fn gap_doc<'src>(context: &mut Context<'_>, start: usize, end: usize, between_items: bool) -> RcDoc<'src> {
    if start > end || end > context.source.len() {
        return RcDoc::nil();
    }

    let trivia = context.trivia.take_gap(start, end);
    if trivia.iter().any(|trivia| trivia.is_comment()) {
        return source_doc(&context.source[start..end]);
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

        let params: Option<Vec<_>> = self
            .params()
            .iter()
            .map(|parameter| parameter.to_doc(context))
            .collect();
        let params = params?;
        let params_doc = if params.is_empty() {
            RcDoc::text("()")
        } else {
            RcDoc::text("(")
                .append(RcDoc::line_())
                .append(RcDoc::intersperse(params, RcDoc::text(",").append(RcDoc::line())))
                .nest(context.config.indent_width as isize)
                .append(RcDoc::line_())
                .append(RcDoc::text(")"))
            // TODO: add group() function when return arguments are too long (firstly collapse arguments, later return values)
        };

        let signature_start = self.span().start;
        let signature_end = self.body().span().start;
        let ret_doc = match self.ret() {
            Some(ty) => RcDoc::text(" -> ").append(type_doc(context, ty, signature_start, signature_end)?),
            None => RcDoc::nil(),
        };

        let sig = vis
            .append(RcDoc::text("fn "))
            .append(name)
            .append(params_doc)
            .append(ret_doc);

        let body = self.body().to_doc(context)?;

        Some(sig.group().append(RcDoc::space()).append(body))
    }
}

impl Doc for FunctionParam {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let span = self.span();
        Some(
            RcDoc::as_string(self.identifier())
                .append(RcDoc::text(": "))
                .append(type_doc(context, self.ty(), span.start, span.end)?),
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
        let span = self.span();
        Some(
            vis.append(RcDoc::text("type "))
                .append(RcDoc::as_string(self.name()))
                .append(RcDoc::text(" = "))
                .append(type_doc(context, self.ty(), span.start, span.end)?)
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

        let span = self.span();
        let payload: Option<Vec<_>> = self
            .payload()
            .iter()
            .map(|ty| type_doc(context, ty, span.start, span.end))
            .collect();
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
        let span = self.span();
        Some(
            RcDoc::text("let ")
                .append(self.pattern().to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(type_doc(context, self.ty(), span.start, span.end)?)
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
            let elements_doc = RcDoc::intersperse(docs?, RcDoc::text(",").append(RcDoc::line()));

            return Some(
                RcDoc::text("(")
                    .append(
                        RcDoc::line_()
                            .append(elements_doc)
                            .nest(context.config.indent_width as isize),
                    )
                    .append(RcDoc::line_())
                    .append(RcDoc::text(")"))
                    .group(),
            );
        }
        if let Some((element, size)) = self.as_array() {
            let size = context.original_type_decimal(size).unwrap_or_else(|| size.to_string());
            return Some(
                RcDoc::text("[")
                    .append(RcDoc::line_())
                    .append(element.to_doc(context)?)
                    .append(RcDoc::text("; "))
                    .append(RcDoc::text(size))
                    .nest(context.config.indent_width as isize)
                    .append(RcDoc::line_())
                    .append(RcDoc::text("]"))
                    .group(),
            );
        }
        if let Some((element, bound)) = self.as_list() {
            let bound = context
                .original_type_decimal(bound.get())
                .unwrap_or_else(|| bound.to_string());
            return Some(
                RcDoc::text("List<")
                    .append(RcDoc::line_())
                    .append(element.to_doc(context)?)
                    .append(RcDoc::text(", "))
                    .append(RcDoc::text(bound))
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
                let docs: Option<Vec<_>> = exprs.iter().map(|expression| expression.to_doc(context)).collect();
                Some(
                    RcDoc::text("(")
                        .append(RcDoc::line_())
                        .append(RcDoc::intersperse(docs?, RcDoc::text(",").append(RcDoc::line())))
                        .nest(context.config.indent_width as isize)
                        .append(RcDoc::line_())
                        .append(RcDoc::text(")"))
                        .group(),
                )
            }
            SingleExpressionInner::Array(exprs) => {
                let docs: Option<Vec<_>> = exprs.iter().map(|expression| expression.to_doc(context)).collect();
                Some(
                    RcDoc::text("[")
                        .append(RcDoc::line_())
                        .append(RcDoc::intersperse(docs?, RcDoc::text(",").append(RcDoc::line())))
                        .nest(context.config.indent_width as isize)
                        .append(RcDoc::line_())
                        .append(RcDoc::text("]"))
                        .group(),
                )
            }
            SingleExpressionInner::List(exprs) => {
                let docs: Option<Vec<_>> = exprs.iter().map(|expression| expression.to_doc(context)).collect();
                Some(
                    RcDoc::text("list![")
                        .append(RcDoc::line_())
                        .append(RcDoc::intersperse(docs?, RcDoc::text(",").append(RcDoc::line())))
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
        let args: Option<Vec<_>> = self.args().iter().map(|argument| argument.to_doc(context)).collect();
        let args = args?;

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

        let (left_arm, right_arm) = match self.left().pattern() {
            MatchPattern::Left(_, _) => (self.left(), self.right()),
            MatchPattern::Right(_, _) => (self.left(), self.right()),
            MatchPattern::None => (self.right(), self.left()),
            MatchPattern::Some(_, _) => (self.left(), self.right()),
            MatchPattern::False => (self.right(), self.left()),
            MatchPattern::True => (self.left(), self.right()),
        };

        let left_arm = left_arm.to_doc(context)?;
        let right_arm = right_arm.to_doc(context)?;

        let body = RcDoc::text("{")
            .append(RcDoc::hardline())
            .append(left_arm.append(RcDoc::text(",")))
            .append(RcDoc::hardline())
            // TODO: maybe add trailing comas to a flag in config?
            .append(right_arm.append(RcDoc::text(",")))
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
        let body = enum_match_body(arms?, context.config.indent_width as isize);

        Some(
            RcDoc::text("match ")
                .append(scrutinee)
                .append(RcDoc::space())
                .append(body),
        )
    }
}

fn enum_match_body<'src>(arms: Vec<RcDoc<'src>>, indent_width: isize) -> RcDoc<'src> {
    if arms.is_empty() {
        RcDoc::text("{}")
    } else {
        RcDoc::text("{")
            .append(RcDoc::hardline())
            .append(RcDoc::intersperse(
                arms.into_iter().map(|arm| arm.append(RcDoc::text(","))),
                RcDoc::hardline(),
            ))
            .nest(indent_width)
            .append(RcDoc::hardline())
            .append(RcDoc::text("}"))
    }
}

impl Doc for MatchArm {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let span = self.span();
        let pat_doc = match self.pattern() {
            MatchPattern::Left(p, ty) => RcDoc::text("Left(")
                .append(p.to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(type_doc(context, ty, span.start, span.end)?)
                .append(RcDoc::text(")"))
                .group(),
            MatchPattern::Right(p, ty) => RcDoc::text("Right(")
                .append(p.to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(type_doc(context, ty, span.start, span.end)?)
                .append(RcDoc::text(")"))
                .group(),
            MatchPattern::None => RcDoc::text("None"),
            MatchPattern::Some(p, ty) => RcDoc::text("Some(")
                .append(p.to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(type_doc(context, ty, span.start, span.end)?)
                .append(RcDoc::text(")"))
                .group(),
            MatchPattern::False => RcDoc::text("false"),
            MatchPattern::True => RcDoc::text("true"),
        };

        Some(
            pat_doc
                .append(RcDoc::text(" => "))
                .append(match_arm_body(self.expression(), context)?)
                .group(),
        )
    }
}

impl Doc for EnumMatchArm {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let mut head = RcDoc::text(self.enum_path_string())
            .append(RcDoc::text("::"))
            .append(RcDoc::as_string(self.variant()));

        if !self.bindings().is_empty() {
            let span = self.span();
            let bindings: Option<Vec<_>> = self
                .bindings()
                .iter()
                .map(|(pattern, ty)| {
                    Some(
                        pattern
                            .to_doc(context)?
                            .append(RcDoc::text(": "))
                            .append(type_doc(context, ty, span.start, span.end)?),
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
                .append(match_arm_body(self.expression(), context)?)
                .group(),
        )
    }
}

fn match_arm_body<'src>(expression: &Expression, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
    let expression = inline_single_expression_block(expression, context);
    let expression_doc = expression.to_doc(context)?;

    match expression.inner() {
        ExpressionInner::Block(..) => Some(expression_doc),
        ExpressionInner::Single(single) if is_match_expression(single) => Some(expression_doc),
        ExpressionInner::Single(..) => Some(
            RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(expression_doc.clone())
                .nest(context.config.indent_width as isize)
                .append(RcDoc::hardline())
                .append(RcDoc::text("}"))
                .flat_alt(expression_doc),
        ),
    }
}

fn is_match_expression(expression: &SingleExpression) -> bool {
    matches!(
        expression.inner(),
        SingleExpressionInner::Match(_) | SingleExpressionInner::EnumMatch(_)
    )
}

fn inline_single_expression_block<'a>(mut expression: &'a Expression, context: &Context<'_>) -> &'a Expression {
    while let ExpressionInner::Block(statements, Some(inner)) = expression.inner() {
        let has_comment = context
            .trivia
            .has_comment_in(expression.span().start, expression.span().end);

        if !statements.is_empty() || has_comment {
            break;
        }
        expression = inner;
    }

    expression
}
