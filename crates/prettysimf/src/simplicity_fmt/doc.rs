use crate::simplicity_fmt::core::{Context, SyntaxKind, Trivia};

use either::Either;
use pretty::RcDoc;
use simplicityhl::error::Span;
use simplicityhl::parse::{
    Assignment, Call, EnumConstruction, EnumDeclaration, EnumMatch, EnumMatchArm, EnumVariant, Expression,
    ExpressionInner, Function, FunctionParam, Item, Match, MatchArm, MatchPattern, Module, Program, SingleExpression,
    SingleExpressionInner, Statement, TypeAlias, UseDecl, UseItems, Visibility,
};
use simplicityhl::pattern::Pattern;
use simplicityhl::types::{AliasedType, TypeDeconstructible};

pub(super) trait Doc {
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
            .map_or(context.source.len(), |span| span.start);

        let prefix = context
            .source
            .get(..context.prefix_end)?
            .trim_start_matches([' ', '\t', '\r', '\n']);
        let mut doc = source_doc(prefix);
        if context.prefix_end < first_start {
            let layout = if prefix.is_empty() {
                GapLayout::Leading
            } else {
                GapLayout::Item
            };
            doc = doc.append(gap_doc(context, context.prefix_end, first_start, layout));
        }
        let mut previous_end = first_start;

        for (index, item) in items.iter().enumerate() {
            let span = item.span()?;
            if span.end <= span.start {
                return None;
            }
            if index > 0 {
                doc = doc.append(gap_doc(context, previous_end, span.start, GapLayout::Item));
            }
            doc = doc.append(item.to_doc(context)?);
            previous_end = span.end;
        }

        doc = doc.append(gap_doc(
            context,
            previous_end,
            context.source.len(),
            GapLayout::Trailing,
        ));
        Some(doc)
    }
}

impl Doc for Item {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GapLayout {
    Leading,
    BeforeToken,
    Tight,
    SoftEmpty,
    Soft,
    Line,
    Item,
    Trailing,
}

struct SpannedDoc<'src> {
    span: Span,
    doc: RcDoc<'src>,
}

fn line_break_count(source: &str) -> usize {
    let mut count = 0;
    let mut bytes = source.bytes().peekable();
    while let Some(byte) = bytes.next() {
        match byte {
            b'\r' => {
                if bytes.peek() == Some(&b'\n') {
                    bytes.next();
                }
                count += 1;
            }
            b'\n' => count += 1,
            _ => {}
        }
    }
    count
}

fn append_hardlines(mut doc: RcDoc<'_>, count: usize) -> RcDoc<'_> {
    for _ in 0..count.min(2) {
        doc = doc.append(RcDoc::hardline());
    }
    doc
}

fn comment_doc<'src>(source: &str, trivia: &Trivia) -> Option<RcDoc<'src>> {
    let text = source.get(trivia.span.start..trivia.span.end)?;
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let line_start = source[..trivia.span.start]
        .rfind(['\n', '\r'])
        .map_or(0, |index| index + 1);
    let indent = source[line_start..trivia.span.start]
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();

    let mut doc = RcDoc::nil();
    for (index, line) in normalized.split('\n').enumerate() {
        if index > 0 {
            doc = doc.append(RcDoc::hardline());
        }
        let line = if index == 0 {
            line
        } else {
            let removable = line
                .bytes()
                .take(indent)
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
            &line[removable..]
        };
        if !line.is_empty() {
            doc = doc.append(RcDoc::text(line.to_owned()));
        }
    }
    Some(doc)
}

/// Render lossless lexer trivia as ordinary pretty-printer documents.
///
/// Callers pass only ranges whose surrounding syntax ownership is known. Any
/// comment in a spanless type, pattern, or name is intentionally left in the
/// cursor and becomes a precise unsupported-comment error after doc creation.
fn gap_doc<'src>(context: &mut Context<'_>, start: usize, end: usize, layout: GapLayout) -> RcDoc<'src> {
    if start > end || end > context.source.len() {
        return RcDoc::nil();
    }

    let trivia = context.trivia.take_gap(start, end);
    let newline_count = trivia.iter().filter(|trivia| trivia.is_newline()).count();
    let comments: Vec<_> = trivia.into_iter().filter(Trivia::is_comment).collect();
    if comments.is_empty() {
        return default_gap_doc(layout, newline_count);
    }

    let mut doc = RcDoc::nil();
    let mut cursor = start;
    for (index, comment) in comments.iter().enumerate() {
        debug_assert!(comment.is_line_comment() || comment.is_block_comment());
        let before = context.source.get(cursor..comment.span.start).unwrap_or_default();
        let breaks = line_break_count(before);
        if layout != GapLayout::Leading || index > 0 {
            if breaks > 0 {
                doc = append_hardlines(doc, breaks);
            } else if comment.span.start > cursor {
                doc = doc.append(RcDoc::space());
            }
        }
        if let Some(rendered) = comment_doc(context.source, comment) {
            doc = doc.append(rendered);
        }
        cursor = comment.span.end;
    }

    let last_is_line = comments.last().is_some_and(Trivia::is_line_comment);
    let trailing_newlines = line_break_count(context.source.get(cursor..end).unwrap_or_default());

    #[allow(clippy::match_same_arms)]
    match layout {
        GapLayout::Leading if trailing_newlines >= 2 => doc.append(RcDoc::hardline()).append(RcDoc::hardline()),
        GapLayout::Leading => doc.append(RcDoc::hardline()),
        GapLayout::BeforeToken | GapLayout::Trailing if last_is_line => doc.append(RcDoc::hardline()),
        GapLayout::Trailing if trailing_newlines > 0 => doc.append(RcDoc::hardline()),
        GapLayout::BeforeToken | GapLayout::Trailing => doc,
        GapLayout::Tight if last_is_line => doc.append(RcDoc::hardline()),
        GapLayout::Tight => doc.append(RcDoc::space()),
        GapLayout::SoftEmpty | GapLayout::Soft if last_is_line => doc.append(RcDoc::hardline()),
        GapLayout::SoftEmpty | GapLayout::Soft => doc.append(RcDoc::line()),
        GapLayout::Line if trailing_newlines >= 2 => doc.append(RcDoc::hardline()).append(RcDoc::hardline()),
        GapLayout::Line => doc.append(RcDoc::hardline()),
        GapLayout::Item if trailing_newlines >= 2 => doc.append(RcDoc::hardline()).append(RcDoc::hardline()),
        GapLayout::Item => doc.append(RcDoc::hardline()),
    }
}

fn default_gap_doc<'src>(layout: GapLayout, newline_count: usize) -> RcDoc<'src> {
    #[allow(clippy::match_same_arms)]
    match layout {
        GapLayout::Leading => RcDoc::nil(),
        GapLayout::BeforeToken => RcDoc::nil(),
        GapLayout::Trailing if newline_count > 0 => RcDoc::hardline(),
        GapLayout::Trailing => RcDoc::nil(),
        GapLayout::Tight => RcDoc::space(),
        GapLayout::SoftEmpty => RcDoc::line_(),
        GapLayout::Soft => RcDoc::line(),
        GapLayout::Line if newline_count >= 2 => RcDoc::hardline().append(RcDoc::hardline()),
        GapLayout::Line => RcDoc::hardline(),
        GapLayout::Item if newline_count >= 2 => RcDoc::hardline().append(RcDoc::hardline()),
        GapLayout::Item => RcDoc::hardline(),
    }
}

fn comma_separated_docs<'src>(context: &mut Context<'_>, docs: Vec<SpannedDoc<'src>>) -> RcDoc<'src> {
    let mut values = docs.into_iter();
    let Some(first) = values.next() else {
        return RcDoc::nil();
    };

    let mut doc = first.doc;
    let mut previous_end = first.span.end;
    for value in values {
        if let Some(comma) = context
            .syntax
            .first_in(SyntaxKind::Comma, previous_end, value.span.start)
            .copied()
        {
            doc = doc
                .append(gap_doc(context, previous_end, comma.start, GapLayout::BeforeToken))
                .append(RcDoc::text(","))
                .append(gap_doc(context, comma.end, value.span.start, GapLayout::Soft));
        } else {
            doc = doc.append(RcDoc::text(",")).append(RcDoc::line());
        }
        doc = doc.append(value.doc);
        previous_end = value.span.end;
    }
    doc
}

fn gap_before_closing_delimiter<'src>(context: &mut Context<'_>, start: usize, close_start: usize) -> RcDoc<'src> {
    if let Some(comma) = context.syntax.first_in(SyntaxKind::Comma, start, close_start).copied() {
        gap_doc(context, start, comma.start, GapLayout::BeforeToken).append(gap_doc(
            context,
            comma.end,
            close_start,
            GapLayout::SoftEmpty,
        ))
    } else {
        gap_doc(context, start, close_start, GapLayout::SoftEmpty)
    }
}

impl Doc for Visibility {
    fn to_doc<'src>(&self, _context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        match self {
            Visibility::Public => Some(RcDoc::text("pub(super) ")),
            Visibility::Private => Some(RcDoc::nil()),
        }
    }
}

impl Doc for Function {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let vis = self.visibility().to_doc(context)?;
        let name = RcDoc::as_string(self.name());

        let signature_end = self.body().span().start;
        let open = context
            .syntax
            .first_in(SyntaxKind::LParen, self.span().start, signature_end)
            .copied()?;
        let params: Option<Vec<_>> = self
            .params()
            .iter()
            .map(|parameter| {
                Some(SpannedDoc {
                    span: *parameter.span(),
                    doc: parameter.to_doc(context)?,
                })
            })
            .collect();
        let params = params?;
        let close_search_start = params.last().map_or(open.end, |parameter| parameter.span.end);
        let close = context
            .syntax
            .first_in(SyntaxKind::RParen, close_search_start, signature_end)
            .copied()?;
        let params_doc = delimited_docs(context, open, close, "(", ")", params, false);

        let signature_start = self.span().start;
        let (ret_doc, body_gap) = match self.ret() {
            Some(ty) => {
                // TODO(comments): the return type has no source span, so
                // comments after the arrow cannot yet be assigned to the type
                // or pre-body gap without guessing.
                let arrow = context
                    .syntax
                    .first_in(SyntaxKind::Arrow, close.end, signature_end)
                    .copied()?;
                (
                    gap_doc(context, close.end, arrow.start, GapLayout::BeforeToken)
                        .append(RcDoc::text(" -> "))
                        .append(type_doc(context, ty, signature_start, signature_end)?),
                    RcDoc::space(),
                )
            }
            None => (
                RcDoc::nil(),
                gap_doc(context, close.end, signature_end, GapLayout::Tight),
            ),
        };

        let sig = vis
            .append(RcDoc::text("fn "))
            .append(name)
            .append(params_doc)
            .append(ret_doc);

        let body = self.body().to_doc(context)?;

        Some(sig.group().append(body_gap).append(body))
    }
}

fn delimited_docs<'src>(
    context: &mut Context<'_>,
    open: Span,
    close: Span,
    open_text: &'static str,
    close_text: &'static str,
    docs: Vec<SpannedDoc<'src>>,
    should_group: bool,
) -> RcDoc<'src> {
    if docs.is_empty() {
        let has_comments = context.trivia.has_comment_in(open.end, close.start);
        if !has_comments {
            return RcDoc::text(open_text).append(RcDoc::text(close_text));
        }

        let doc = RcDoc::text(open_text)
            .append(
                gap_doc(context, open.end, close.start, GapLayout::SoftEmpty)
                    .nest(context.config.indent_width.cast_signed()),
            )
            .append(RcDoc::text(close_text));
        return if should_group { doc.group() } else { doc };
    }

    let first_start = docs.first().map_or(open.end, |doc| doc.span.start);
    let last_end = docs.last().map_or(open.end, |doc| doc.span.end);
    let inner = gap_doc(context, open.end, first_start, GapLayout::SoftEmpty)
        .append(comma_separated_docs(context, docs))
        .append(gap_before_closing_delimiter(context, last_end, close.start));

    let doc = RcDoc::text(open_text)
        .append(inner.nest(context.config.indent_width.cast_signed()))
        .append(RcDoc::text(close_text));
    if should_group { doc.group() } else { doc }
}

impl Doc for FunctionParam {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        // TODO(comments): SimplicityHL exposes only the aggregate parameter
        //  span, not separate identifier/type spans. Comments inside either
        //  component stay unattached until those boundaries are available.
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
        // TODO(comments): use-path segments and imported names do not expose
        //  individual spans. Comments inside this declaration deliberately
        //  remain unattached and produce an unsupported-comment error.
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
                    .nest(context.config.indent_width.cast_signed())
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
        // TODO(comments): aliases expose a declaration span but not separate
        //  name and structural-type component spans.
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
        let variants: Option<Vec<_>> = self
            .variants()
            .iter()
            .map(|variant| {
                Some(SpannedDoc {
                    span: *variant.span(),
                    doc: variant.to_doc(context)?,
                })
            })
            .collect();
        let variants = variants?;
        let open = context
            .syntax
            .first_in(SyntaxKind::LBrace, self.span().start, self.span().end)
            .copied()?;
        let close = context
            .syntax
            .last_in(SyntaxKind::RBrace, open.end, self.span().end)
            .copied()?;

        let body = if variants.is_empty() {
            let has_comments = context.trivia.has_comment_in(open.end, close.start);
            if has_comments {
                RcDoc::text("{")
                    .append(
                        gap_doc(context, open.end, close.start, GapLayout::Line)
                            .nest(context.config.indent_width.cast_signed()),
                    )
                    .append(RcDoc::text("}"))
            } else {
                RcDoc::text("{}")
            }
        } else {
            let first = variants.first()?;
            let first_limit = variants.get(1).map_or(close.start, |variant| variant.span.start);
            let first_comma = context
                .syntax
                .first_in(SyntaxKind::Comma, first.span.end, first_limit)
                .copied();
            let mut inner = gap_doc(context, open.end, first.span.start, GapLayout::Line)
                .append(first.doc.clone())
                .append(gap_doc(
                    context,
                    first.span.end,
                    first_comma.map_or(first.span.end, |comma| comma.start),
                    GapLayout::BeforeToken,
                ))
                .append(RcDoc::text(","));
            let mut previous_end = first_comma.map_or(first.span.end, |comma| comma.end);

            for (index, variant) in variants.iter().enumerate().skip(1) {
                inner = inner
                    .append(gap_doc(context, previous_end, variant.span.start, GapLayout::Line))
                    .append(variant.doc.clone());
                let limit = variants.get(index + 1).map_or(close.start, |next| next.span.start);
                let comma = context
                    .syntax
                    .first_in(SyntaxKind::Comma, variant.span.end, limit)
                    .copied();
                inner = inner
                    .append(gap_doc(
                        context,
                        variant.span.end,
                        comma.map_or(variant.span.end, |comma| comma.start),
                        GapLayout::BeforeToken,
                    ))
                    .append(RcDoc::text(","));
                previous_end = comma.map_or(variant.span.end, |comma| comma.end);
            }

            inner = inner.append(gap_doc(context, previous_end, close.start, GapLayout::Line));
            RcDoc::text("{")
                .append(inner.nest(context.config.indent_width.cast_signed()))
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
        // TODO(comments): enum payload types have no individual source spans.
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
                .nest(context.config.indent_width.cast_signed())
                .append(RcDoc::line_())
                .append(RcDoc::text(")"))
                .group(),
        )
    }
}

impl Doc for Module {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let vis = self.visibility().to_doc(context)?;
        let open = context
            .syntax
            .first_in(SyntaxKind::LBrace, self.span().start, self.span().end)
            .copied()?;
        let close = context
            .syntax
            .last_in(SyntaxKind::RBrace, open.end, self.span().end)
            .copied()?;
        let items: Option<Vec<_>> = self
            .items()
            .iter()
            .map(|item| {
                Some(SpannedDoc {
                    span: *item.span()?,
                    doc: item.to_doc(context)?,
                })
            })
            .collect();
        let items = items?;
        let body = if items.is_empty() {
            let has_comments = context.trivia.has_comment_in(open.end, close.start);
            if has_comments {
                RcDoc::text("{")
                    .append(
                        gap_doc(context, open.end, close.start, GapLayout::Line)
                            .nest(context.config.indent_width.cast_signed()),
                    )
                    .append(RcDoc::text("}"))
            } else {
                RcDoc::text("{}")
            }
        } else {
            let first = items.first()?;
            let mut inner = gap_doc(context, open.end, first.span.start, GapLayout::Line).append(first.doc.clone());
            let mut previous_end = first.span.end;
            for item in items.iter().skip(1) {
                inner = inner
                    .append(gap_doc(context, previous_end, item.span.start, GapLayout::Item))
                    .append(item.doc.clone());
                previous_end = item.span.end;
            }
            inner = inner.append(gap_doc(context, previous_end, close.start, GapLayout::Line));
            RcDoc::text("{")
                .append(inner.nest(context.config.indent_width.cast_signed()))
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
                let span = *self.span();
                let open = context
                    .syntax
                    .first_in(SyntaxKind::LBrace, span.start, span.end)
                    .copied()?;
                let close = context
                    .syntax
                    .last_in(SyntaxKind::RBrace, open.end, span.end)
                    .copied()?;

                let mut docs = Vec::with_capacity(stmts.len() + usize::from(expr.is_some()));
                for (index, statement) in stmts.iter().enumerate() {
                    let separator_limit = stmts
                        .get(index + 1)
                        .map(|next| next.span().start)
                        .or_else(|| expr.as_ref().map(|trailing| trailing.span().start))
                        .unwrap_or(close.start);
                    docs.push(statement_doc(context, statement, separator_limit)?);
                }
                if let Some(trailing) = expr {
                    docs.push(SpannedDoc {
                        span: *trailing.span(),
                        doc: trailing.to_doc(context)?,
                    });
                }

                if docs.is_empty() {
                    let has_comments = context.trivia.has_comment_in(open.end, close.start);
                    if !has_comments {
                        return Some(RcDoc::text("{}"));
                    }

                    return Some(
                        RcDoc::text("{")
                            .append(
                                gap_doc(context, open.end, close.start, GapLayout::Line)
                                    .nest(context.config.indent_width.cast_signed()),
                            )
                            .append(RcDoc::text("}")),
                    );
                }

                let first = docs.first()?;
                let mut inner = gap_doc(context, open.end, first.span.start, GapLayout::Line).append(first.doc.clone());
                let mut previous_end = first.span.end;
                for value in docs.iter().skip(1) {
                    inner = inner
                        .append(gap_doc(context, previous_end, value.span.start, GapLayout::Line))
                        .append(value.doc.clone());
                    previous_end = value.span.end;
                }
                inner = inner.append(gap_doc(context, previous_end, close.start, GapLayout::Line));

                Some(
                    RcDoc::text("{")
                        .append(inner.nest(context.config.indent_width.cast_signed()))
                        .append(RcDoc::text("}")),
                )
            }
        }
    }
}

fn statement_doc<'src>(
    context: &mut Context<'_>,
    statement: &Statement,
    separator_limit: usize,
) -> Option<SpannedDoc<'src>> {
    let span = *statement.span();
    let semicolon = context
        .syntax
        .first_in(SyntaxKind::Semi, span.end, separator_limit)
        .copied()?;
    let doc = statement
        .to_doc(context)?
        .append(gap_doc(context, span.end, semicolon.start, GapLayout::BeforeToken))
        .append(RcDoc::text(";"));

    Some(SpannedDoc {
        span: Span::new(span.file_id, span.start..semicolon.end),
        doc,
    })
}

impl Doc for Statement {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        match self {
            Statement::Assignment(a) => a.to_doc(context),
            Statement::Expression(e) => e.to_doc(context),
        }
    }
}

impl Doc for Assignment {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        // TODO(comments): assignment patterns and type syntax are semantic
        //  values without component spans. Their internal comments stay
        //  unattached until SimplicityHL exposes those boundaries.
        let span = self.span();
        let equals = context
            .syntax
            .last_in(SyntaxKind::Eq, span.start, self.expression().span().start)
            .copied()?;
        Some(
            RcDoc::text("let ")
                .append(self.pattern().to_doc(context)?)
                .append(RcDoc::text(": "))
                .append(type_doc(context, self.ty(), span.start, span.end)?)
                .append(RcDoc::text(" ="))
                .append(gap_doc(
                    context,
                    equals.end,
                    self.expression().span().start,
                    GapLayout::Tight,
                ))
                .append(self.expression().to_doc(context)?)
                .group(),
        )
    }
}

impl Doc for AliasedType {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        // TODO(comments): AliasedType has no nested source spans. The formatter
        //  can structure it, but cannot safely assign trivia between components.
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
                    .nest(context.config.indent_width.cast_signed())
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
                    .nest(context.config.indent_width.cast_signed())
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
                            .nest(context.config.indent_width.cast_signed()),
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
                    .nest(context.config.indent_width.cast_signed())
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
                    .nest(context.config.indent_width.cast_signed())
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
        // TODO(comments): Pattern has no source spans, so comments inside
        // tuple and array patterns cannot be matched to child documents yet.
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
                                .nest(context.config.indent_width.cast_signed()),
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
                                .nest(context.config.indent_width.cast_signed()),
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
            SingleExpressionInner::Either(Either::Left(e)) => wrapper_doc(self, context, "Left", e),
            SingleExpressionInner::Either(Either::Right(e)) => wrapper_doc(self, context, "Right", e),
            SingleExpressionInner::Option(Some(e)) => wrapper_doc(self, context, "Some", e),
            SingleExpressionInner::Option(None) => Some(RcDoc::text("None")),
            SingleExpressionInner::Boolean(b) => Some(RcDoc::as_string(b)),
            SingleExpressionInner::Decimal(d) => Some(RcDoc::as_string(d)),
            SingleExpressionInner::Binary(b) => Some(RcDoc::text("0b").append(RcDoc::as_string(b))),
            SingleExpressionInner::Hexadecimal(h) => Some(RcDoc::text("0x").append(RcDoc::as_string(h))),
            SingleExpressionInner::Witness(w) => Some(RcDoc::text("witness::").append(RcDoc::as_string(w))),
            SingleExpressionInner::Parameter(p) => Some(RcDoc::text("param::").append(RcDoc::as_string(p))),
            SingleExpressionInner::Variable(v) => Some(RcDoc::as_string(v)),
            SingleExpressionInner::Call(c) => c.to_doc(context),
            SingleExpressionInner::Expression(e) => wrapper_doc(self, context, "", e),
            SingleExpressionInner::Match(m) => m.to_doc(context),
            SingleExpressionInner::EnumMatch(m) => m.to_doc(context),
            SingleExpressionInner::EnumConstruction(construction) => construction.to_doc(context),
            SingleExpressionInner::Tuple(exprs) => {
                sequence_doc(self, context, SyntaxKind::LParen, SyntaxKind::RParen, "(", ")", exprs)
            }
            SingleExpressionInner::Array(exprs) => sequence_doc(
                self,
                context,
                SyntaxKind::LBracket,
                SyntaxKind::RBracket,
                "[",
                "]",
                exprs,
            ),
            SingleExpressionInner::List(exprs) => sequence_doc(
                self,
                context,
                SyntaxKind::LBracket,
                SyntaxKind::RBracket,
                "list![",
                "]",
                exprs,
            ),
        }
    }
}

fn wrapper_doc<'src>(
    single: &SingleExpression,
    context: &mut Context<'_>,
    prefix: &'static str,
    expression: &Expression,
) -> Option<RcDoc<'src>> {
    let span = *single.span();
    let open = context
        .syntax
        .first_in(SyntaxKind::LParen, span.start, expression.span().start)
        .copied()?;
    let close = context
        .syntax
        .last_in(SyntaxKind::RParen, expression.span().end, span.end)
        .copied()?;
    let docs = vec![SpannedDoc {
        span: *expression.span(),
        doc: expression.to_doc(context)?,
    }];
    Some(RcDoc::text(prefix).append(delimited_docs(context, open, close, "(", ")", docs, true)))
}

fn sequence_doc<'src>(
    single: &SingleExpression,
    context: &mut Context<'_>,
    open_kind: SyntaxKind,
    close_kind: SyntaxKind,
    open_text: &'static str,
    close_text: &'static str,
    expressions: &[Expression],
) -> Option<RcDoc<'src>> {
    let span = *single.span();
    let open = context.syntax.first_in(open_kind, span.start, span.end).copied()?;
    let close = context.syntax.last_in(close_kind, open.end, span.end).copied()?;
    let docs: Option<Vec<_>> = expressions
        .iter()
        .map(|expression| {
            Some(SpannedDoc {
                span: *expression.span(),
                doc: expression.to_doc(context)?,
            })
        })
        .collect();
    Some(delimited_docs(context, open, close, open_text, close_text, docs?, true))
}

impl Doc for Call {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        // TODO(comments): CallName can contain structural types, but has no
        // source span. Comments inside that name/type portion stay unsupported.
        let name_doc = RcDoc::as_string(self.name());
        let span = *self.span();
        let first_arg_start = self.args().first().map_or(span.end, |argument| argument.span().start);
        let open = context
            .syntax
            .last_in(SyntaxKind::LParen, span.start, first_arg_start)
            .copied()?;
        let close = context
            .syntax
            .last_in(SyntaxKind::RParen, open.end, span.end)
            .copied()?;
        let args: Option<Vec<_>> = self
            .args()
            .iter()
            .map(|argument| {
                Some(SpannedDoc {
                    span: *argument.span(),
                    doc: argument.to_doc(context)?,
                })
            })
            .collect();

        Some(name_doc.append(delimited_docs(context, open, close, "(", ")", args?, true)))
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

        // TODO(comments): enum paths and variant identifiers have no
        // individual spans, so comments inside the head stay unsupported.
        let span = *self.span();
        let first_arg_start = self.args().first()?.span().start;
        let open = context
            .syntax
            .last_in(SyntaxKind::LParen, span.start, first_arg_start)
            .copied()?;
        let close = context
            .syntax
            .last_in(SyntaxKind::RParen, open.end, span.end)
            .copied()?;
        let args: Option<Vec<_>> = self
            .args()
            .iter()
            .map(|argument| {
                Some(SpannedDoc {
                    span: *argument.span(),
                    doc: argument.to_doc(context)?,
                })
            })
            .collect();
        Some(head.append(delimited_docs(context, open, close, "(", ")", args?, true)))
    }
}

impl Doc for Match {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let scrutinee = self.scrutinee().to_doc(context)?;
        let span = *self.span();
        let open = context
            .syntax
            .first_in(SyntaxKind::LBrace, self.scrutinee().span().end, span.end)
            .copied()?;
        let close = context
            .syntax
            .last_in(SyntaxKind::RBrace, open.end, span.end)
            .copied()?;

        let (left_arm, right_arm) = (self.left(), self.right());

        let left = SpannedDoc {
            span: *left_arm.span(),
            doc: left_arm.to_doc(context)?.append(RcDoc::text(",")),
        };
        let right = SpannedDoc {
            span: *right_arm.span(),
            doc: right_arm.to_doc(context)?.append(RcDoc::text(",")),
        };
        let body = if left.span.start < right.span.start {
            match_body_doc(context, open, close, &[left, right])?
        } else {
            // TODO(comments): option/bool arms are reordered canonically.
            // Inter-arm comments cannot follow that reorder safely until trivia
            // has explicit arm ownership, so those comments remain unattached.
            RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(left.doc)
                .append(RcDoc::hardline())
                .append(right.doc)
                .nest(context.config.indent_width.cast_signed())
                .append(RcDoc::hardline())
                .append(RcDoc::text("}"))
        };

        Some(
            RcDoc::text("match ")
                .append(scrutinee)
                .append(gap_doc(
                    context,
                    self.scrutinee().span().end,
                    open.start,
                    GapLayout::Tight,
                ))
                .append(body),
        )
    }
}

impl Doc for EnumMatch {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        let scrutinee = self.scrutinee().to_doc(context)?;
        let span = *self.span();
        let open = context
            .syntax
            .first_in(SyntaxKind::LBrace, self.scrutinee().span().end, span.end)
            .copied()?;
        let close = context
            .syntax
            .last_in(SyntaxKind::RBrace, open.end, span.end)
            .copied()?;
        let arms: Option<Vec<_>> = self
            .arms()
            .iter()
            .map(|arm| {
                Some(SpannedDoc {
                    span: *arm.span(),
                    doc: arm.to_doc(context)?.append(RcDoc::text(",")),
                })
            })
            .collect();
        let arms = arms?;
        let body = if arms.is_empty() {
            RcDoc::text("{}")
        } else {
            match_body_doc(context, open, close, &arms)?
        };

        Some(
            RcDoc::text("match ")
                .append(scrutinee)
                .append(gap_doc(
                    context,
                    self.scrutinee().span().end,
                    open.start,
                    GapLayout::Tight,
                ))
                .append(body),
        )
    }
}

fn match_body_doc<'src>(
    context: &mut Context<'_>,
    open: Span,
    close: Span,
    arms: &[SpannedDoc<'src>],
) -> Option<RcDoc<'src>> {
    let first = arms.first()?;
    let mut inner = gap_doc(context, open.end, first.span.start, GapLayout::Line).append(first.doc.clone());
    let mut previous_end = first.span.end;
    for arm in arms.iter().skip(1) {
        inner = inner
            .append(gap_doc(context, previous_end, arm.span.start, GapLayout::Line))
            .append(arm.doc.clone());
        previous_end = arm.span.end;
    }
    inner = inner.append(gap_doc(context, previous_end, close.start, GapLayout::Line));
    Some(
        RcDoc::text("{")
            .append(inner.nest(context.config.indent_width.cast_signed()))
            .append(RcDoc::text("}")),
    )
}

fn match_arm_tail_doc<'src>(context: &mut Context<'_>, expression_end: usize, arm_end: usize) -> RcDoc<'src> {
    if let Some(comma) = context
        .syntax
        .first_in(SyntaxKind::Comma, expression_end, arm_end)
        .copied()
    {
        gap_doc(context, expression_end, comma.start, GapLayout::BeforeToken).append(gap_doc(
            context,
            comma.end,
            arm_end,
            GapLayout::BeforeToken,
        ))
    } else {
        gap_doc(context, expression_end, arm_end, GapLayout::BeforeToken)
    }
}

impl Doc for MatchArm {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        // TODO(comments): MatchPattern and its type carry no component spans.
        // Comments inside the arm head stay unsupported until they do.
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

        let arrow = context
            .syntax
            .last_in(SyntaxKind::FatArrow, span.start, self.expression().span().start)
            .copied()?;
        Some(
            pat_doc
                .append(RcDoc::text(" =>"))
                .append(gap_doc(
                    context,
                    arrow.end,
                    self.expression().span().start,
                    GapLayout::Tight,
                ))
                .append(match_arm_body(self.expression(), context)?)
                .append(match_arm_tail_doc(context, self.expression().span().end, span.end))
                .group(),
        )
    }
}

impl Doc for EnumMatchArm {
    fn to_doc<'src>(&self, context: &mut Context<'_>) -> Option<RcDoc<'src>> {
        // TODO(comments): enum arm paths, bindings, patterns, and types do not
        // expose component spans. Comments inside the head stay unsupported.
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
                .nest(context.config.indent_width.cast_signed())
                .append(RcDoc::line_())
                .append(RcDoc::text(")"))
                .group();
        }

        let arrow = context
            .syntax
            .last_in(SyntaxKind::FatArrow, self.span().start, self.expression().span().start)
            .copied()?;
        Some(
            head.append(RcDoc::text(" =>"))
                .append(gap_doc(
                    context,
                    arrow.end,
                    self.expression().span().start,
                    GapLayout::Tight,
                ))
                .append(match_arm_body(self.expression(), context)?)
                .append(match_arm_tail_doc(
                    context,
                    self.expression().span().end,
                    self.span().end,
                ))
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
                .nest(context.config.indent_width.cast_signed())
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
