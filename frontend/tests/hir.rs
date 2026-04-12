use common::{ASTExpression, ASTExpressionKind, Span};
use frontend::hir::{SlynxHir, types::HirType};

fn generate_hir(kind: ASTExpressionKind) -> ASTExpression {
    let span = Span { start: 0, end: 0 };
    ASTExpression { kind, span }
}

#[test]
fn test_hir_tuple() {
    let tuple_ast = generate_hir(ASTExpressionKind::Tuple(vec![
        generate_hir(ASTExpressionKind::FloatLiteral(1.9)),
        generate_hir(ASTExpressionKind::IntLiteral(42)),
    ]));
    let mut hir = SlynxHir::new();
    let result = hir.resolve_expr(tuple_ast, None);
    assert!(result.is_ok(), "err in tuple hir: {:?}", result);
    let tuple_hir = result.unwrap();
    match tuple_hir.kind {
        frontend::hir::definitions::HirExpressionKind::Tuple(ref elements) => {
            assert_eq!(elements.len(), 2);
        }
        _ => panic!("expected HirExpressionKind::Tuple"),
    }
    match hir.types_module.get_type(&tuple_hir.ty) {
        HirType::Tuple { fields } => {
            assert_eq!(fields.len(), 2);
        }
        _ => panic!("expected HirType::Tuple"),
    }
}
