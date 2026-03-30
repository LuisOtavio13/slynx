# Implementar Assigns no Middleend (SlynxIR)

## Contexto

O pipeline ativo (`SlynxContext::compile()` em `src/context.rs:296`) usa o `SlynxIR` (IR antigo). O caso `HirStatementKind::Assign` em `middleend/src/ir/instructions.rs:26-29` está `unimplemented!()`.

O `IntermediateRepr` (IR novo) já tem `generate_assign()` e `generate_place()` funcionando para `Identifier` e `FieldAccess`, mas o pipeline principal não o usa.

## Casos a suportar

| Código Slynx | HIR LHS | IR Strategy |
|---|---|---|
| `a = 12` | `Identifier(var_id)` | `Write(slot, value)` direto |
| `v.x = 44` | `FieldAccess { expr: Identifier(v), field_index }` | Read v → SetField → Write v |
| `func().a = 77` | `FieldAccess { expr: FunctionCall {..}, field_index }` | Call → SetField (resultado descartado) |

## Mudanças

### 1. `middleend/src/ir/instructions.rs` — Método auxiliar `get_slot_for_place`

Adicionar um método que resolve uma expressão LHS para um `(IRPointer<Value, 1>, IRTypeId)` — o slot da variável e o tipo do struct contido nele.

```rust
fn get_slot_for_place(
    &self,
    lhs: &HirExpression,
    temp: &TempIRData,
) -> Result<(IRPointer<Value, 1>, IRTypeId), IRError> {
    match &lhs.kind {
        HirExpressionKind::Identifier(id) => {
            let slot = temp.get_variable(*id)
                .ok_or(IRError::UnrecognizedVariable(*id))?;
            let ty = self.get_type_of_value(slot.clone(), temp);
            Ok((slot, ty))
        }
        HirExpressionKind::FieldAccess { expr: parent, field_index } => {
            // Recursivamente pega o slot do parent
            let (parent_slot, parent_ty) = self.get_slot_for_place(parent, temp)?;
            // Lê o struct atual do slot do parent
            let parent_value = self.get_value(parent_slot.clone());
            let parent_ptr = self.insert_value(parent_value);
            // Pega o tipo do field específico
            let field_ty = self.types.get_field_type(parent_ty, *field_index);
            Ok((parent_slot, field_ty))
        }
        _ => Err(IRError::UnrecognizedExpression(/* ou panic */)),
    }
}
```

### 2. `middleend/src/ir/instructions.rs` — Handler `Assign` em `get_instruction`

Substituir o `unimplemented!()`:

```rust
HirStatementKind::Assign { lhs, value } => {
    let value_ptr = self.get_value_for(value, temp)?;

    match &lhs.kind {
        // Caso simples: a = 12
        HirExpressionKind::Identifier(id) => {
            let slot = temp.get_variable(*id)
                .expect("Variable not found");
            self.write(slot, value_ptr, temp);
        }
        // Caso field access: v.x = 44 ou func().a = 77
        HirExpressionKind::FieldAccess { expr: parent, field_index } => {
            // 1. Gera o valor do parent (avalia expressão complexa como func())
            let parent_value = self.get_value_for(parent, temp)?;
            let parent_ty = self.get_type_of_value(parent_value.clone(), temp);

            // 2. SetField: cria novo struct com o field modificado
            let target = self.insert_value(self.get_value(parent_value));
            let val = self.insert_value(self.get_value(value_ptr));
            let target = IRPointer::<Value, 2>::new(
                self.values.len() - 2, // pointer to [parent_val, new_val]
                2
            );
            let setfield_instr = self.insert_instruction(
                temp.current_label(),
                Instruction::setfield(*field_index, target, parent_ty),
            );

            // 3. Se o parent era um Identifier (variável), escreve de volta no slot
            if let HirExpressionKind::Identifier(id) = &parent.kind {
                let slot = temp.get_variable(*id)
                    .expect("Variable not found");
                let new_value = self.insert_value(
                    Value::Instruction(setfield_instr)
                );
                self.write(slot, new_value, temp);
            }
            // Se era func().a = 77, o resultado do SetField fica no ar
            // (side-effect only, sem slot para escrever de volta)
        }
        _ => unreachable!("LHS must be Identifier or FieldAccess"),
    }
    Ok(None)
}
```

### 3. Helper `get_field_type` em `middleend/src/types/mod.rs` (IRTypes)

Adicionar ao `IRTypes` (que tem acesso mutável a structs/components):

```rust
pub fn get_field_type(&self, ty: IRTypeId, field_index: usize) -> IRTypeId {
    let ir_type = self.types[ty.0].clone(); // clone barato, IRType é Copy-like
    match ir_type {
        IRType::Struct(sid) => self.structs[sid.0].get_fields()[field_index],
        IRType::Component(cid) => self.components[cid.0].fields[field_index],
        _ => panic!("Expected struct or component type for field access, got {:?}", ir_type),
    }
}
```

Nota: `IRStruct` já tem `get_fields() -> &[IRTypeId]`. `IRComponent.fields` é `pub(crate)`, então acessível de dentro do crate. Alternativamente, adicionar `get_fields()` ao `IRComponent` para consistência.

### 4. Verificar se `SetField` com `IRPointer<Value, 2>` funciona corretamente

O construtor `Instruction::setfield()` espera `IRPointer<Value, 2>` (dois values consecutivos no vetor `values`). Precisamos garantir que os dois values (parent + new_value) são inseridos consecutivamente antes de criar o pointer.

## Arquivos modificados

| Arquivo | Mudança |
|---|---|
| `middleend/src/ir/instructions.rs` | Substituir `unimplemented!()`, adicionar lógica de Assign |
| `middleend/src/types/mod.rs` | Adicionar `get_field_type` ao `IRTypes` |
| `middleend/src/types/components.rs` | (Opcional) Adicionar `get_fields()` ao `IRComponent` |

## Validação

1. `cargo build` — verificar compilação
2. Criar um arquivo de teste `.slynx` com os 4 casos de assign
3. Rodar o compilador e verificar a saída `.sir` não tem mais panic
