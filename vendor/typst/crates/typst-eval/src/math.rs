use ecow::eco_format;
use typst_library::World;
use typst_library::diag::{At, SourceResult};
use typst_library::foundations::{Content, NativeElement, Symbol, SymbolElem, Value};
use typst_library::math::{
    AlignPointElem, AttachElem, EquationElem, FracElem, LrElem, PrimesElem, RootElem,
};
use typst_library::text::TextElem;
use typst_syntax::ast::{self, AstNode, MathTextKind};

use crate::{Eval, Vm};

impl Eval for ast::Equation<'_> {
    type Output = Content;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        let body = self.body().eval(vm)?;
        let block = self.block();
        Ok(EquationElem::new(body).with_block(block).pack())
    }
}

impl Eval for ast::Math<'_> {
    type Output = Content;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        Ok(Content::sequence(
            self.exprs()
                .map(|expr| expr.eval_display(vm))
                .collect::<SourceResult<Vec<_>>>()?,
        ))
    }
}

impl Eval for ast::MathText<'_> {
    type Output = Content;

    fn eval(self, _: &mut Vm) -> SourceResult<Self::Output> {
        match self.get() {
            MathTextKind::Grapheme(text) => Ok(SymbolElem::packed(text.clone())),
            MathTextKind::Number(text) => Ok(TextElem::packed(text.clone())),
        }
    }
}

impl Eval for ast::MathIdent<'_> {
    type Output = Value;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        let span = self.span();
        Ok(vm
            .scopes
            .get_in_math(&self)
            .at(span)?
            .read_checked((&mut vm.engine, span))
            .clone())
    }
}

impl Eval for ast::MathFieldAccess<'_> {
    type Output = Value;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        let target = self.target().eval(vm)?;
        let field = self.field();
        crate::code::access_field(vm, target, field.as_str(), field.span())
    }
}

impl Eval for ast::MathAccess<'_> {
    type Output = Value;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        let value = match self {
            ast::MathAccess::MathIdent(ident) => ident.eval(vm)?,
            ast::MathAccess::MathFieldAccess(access) => access.eval(vm)?,
        };
        // We need to call `trace_at` for the value because we did not evaluate
        // via `ast::Expr::eval()`.
        vm.trace_at(self.span(), &value);
        Ok(value)
    }
}

impl Eval for ast::MathShorthand<'_> {
    type Output = Value;

    fn eval(self, _: &mut Vm) -> SourceResult<Self::Output> {
        Ok(Value::Symbol(Symbol::runtime_char(self.get())))
    }
}

impl Eval for ast::MathAlignPoint<'_> {
    type Output = Content;

    fn eval(self, _: &mut Vm) -> SourceResult<Self::Output> {
        Ok(AlignPointElem::shared().clone())
    }
}

impl Eval for ast::MathDelimited<'_> {
    type Output = Content;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        let open = self.open().eval_display(vm)?;
        let body = self.body().eval(vm)?;
        let close = self.close().eval_display(vm)?;
        Ok(LrElem::new(open + body + close).pack())
    }
}

impl Eval for ast::MathAttach<'_> {
    type Output = Content;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        let base = self.base().eval_display(vm)?;
        let mut elem = AttachElem::new(base);

        if let Some(expr) = self.top() {
            elem.t.set(Some(expr.eval_display(vm)?));
        }

        // Always attach primes in scripts style (not limits style),
        // i.e. at the top-right corner.
        if let Some(primes) = self.primes() {
            elem.tr.set(Some(primes.eval(vm)?));
        }

        if let Some(expr) = self.bottom() {
            elem.b.set(Some(expr.eval_display(vm)?));
        }

        Ok(elem.pack())
    }
}

impl Eval for ast::MathPrimes<'_> {
    type Output = Content;

    fn eval(self, _: &mut Vm) -> SourceResult<Self::Output> {
        Ok(PrimesElem::new(self.count()).pack())
    }
}

impl Eval for ast::MathFrac<'_> {
    type Output = Content;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        let num_expr = self.num();
        let num = num_expr.eval_display(vm)?;
        let denom_expr = self.denom();
        let denom = denom_expr.eval_display(vm)?;

        let num_depar =
            matches!(num_expr, ast::Expr::Math(math) if math.was_deparenthesized());
        let denom_depar =
            matches!(denom_expr, ast::Expr::Math(math) if math.was_deparenthesized());

        Ok(FracElem::new(num, denom)
            .with_num_deparenthesized(num_depar)
            .with_denom_deparenthesized(denom_depar)
            .pack())
    }
}

impl Eval for ast::MathRoot<'_> {
    type Output = Content;

    fn eval(self, vm: &mut Vm) -> SourceResult<Self::Output> {
        // Use `TextElem` to match `MathTextKind::Number` above.
        let index = self.index().map(|i| TextElem::packed(eco_format!("{i}")));
        let radicand = self.radicand().eval_display(vm)?;
        Ok(RootElem::new(radicand).with_index(index).pack())
    }
}

trait ExprExt {
    fn eval_display(&self, vm: &mut Vm) -> SourceResult<Content>;
}

impl ExprExt for ast::Expr<'_> {
    fn eval_display(&self, vm: &mut Vm) -> SourceResult<Content> {
        let content = self.eval(vm)?.display().spanned(self.span());
        if !vm.world().editor_math_origin(self.span()) {
            return Ok(content);
        }
        // visual-typst source provenance: zero-size existing math tags, not a
        // box (which would change atom classes, spacing, or stretch behaviour).
        // These markers are not queryable and never become visible SVG nodes.
        use typst_library::introspection::{Location, MetadataElem, Tag, TagElem, TagFlags};
        let key = typst_utils::hash128(&("visual-typst-origin-v1", self.span()));
        let location = Location::new(key);
        let mut marker = MetadataElem::new(Value::Str("visual-typst-origin-v1".into()))
            .pack().spanned(self.span());
        marker.set_location(location);
        let flags = TagFlags { introspectable: false, tagged: false };
        Ok(Content::sequence([
            TagElem::packed(Tag::Start(marker, flags)),
            content,
            TagElem::packed(Tag::End(location, key, flags)),
        ]))
    }
}
