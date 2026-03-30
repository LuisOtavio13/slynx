# Implementação de Assigns no SlynxIR

## Data
2026-03-28

## Resumo
Implementação dos assigns no `SlynxIR` (IR antigo), substituindo o `unimplemented!()` no handler de `Assign` em `middleend/src/ir/instructions.rs`.

## Mudanças

### 1. `middleend/src/types/mod.rs` — `get_field_type` (linha 67)
Adicionado método `get_field_type(&self, ty: IRTypeId, field_index: usize) -> IRTypeId` ao `IRTypes`.

Resolve o tipo de um field específico em structs e components:
- `IRType::Struct(sid)` → `self.structs[sid.0].get_fields()[field_index]`
- `IRType::Component(cid)` → `self.components[cid.0].fields[field_index]`
- Outros → panic (não deveria ocorrer)

### 2. `middleend/src/ir/instructions.rs` — Handler `Assign` (linha 28)
Substituído `unimplemented!()` por lógica de read-modify-write.

**Caso `Identifier`** (`a = 12`):
1. Busca o slot da variável via `temp.get_variable(id)`
2. Extrai o `IRPointer<Slot, 1>` do `Value::Slot` armazenado
3. Emite `Write(slot, value_ptr)`

**Caso `FieldAccess`** (`v.x = 44`):
1. Avalia o parent com `get_value_for(parent, temp)`
2. Obtém o tipo do parent com `get_type_of_value`
3. Insere parent_value e rhs_value consecutivamente no values vec
4. Cria `IRPointer<Value, 2>` para os dois operands
5. Emite `SetField(field_index, ptr, parent_ty)`
6. Se parent era `Identifier`, extrai slot e escreve de volta com `Write`

**Caso `FieldAccess` com parent não-Identifier** (`func().a = 77`):
- O SetField é emitido mas sem write-back (resultado descartado)

## Descobertas importantes

- `temp.get_variable()` retorna `IRPointer<Value, 1>` (ponteiro para `Value::Slot`), não `IRPointer<Slot, 1>`. É necessário fazer unwrap do `Value::Slot` para obter o slot pointer.
- `IRTypes::get_object_type` requer `&mut self`, mas `get_field_type` funciona com `&self` pois só faz leitura.
- `IRComponent.fields` é `pub(crate)`, acessível diretamente dentro do crate.
- `SetField` espera `IRPointer<Value, 2>` — dois values consecutivos. O padrão é inserir ambos com `insert_value` e depois construir o pointer com `IRPointer::new(self.values.len() - 2, 2)`.
- `HirExpressionKind::FieldAccess.field_index` é `usize` (índice já resolvido pelo type checker), não string.
- Nested FieldAccess (`v.x.y = 5`) não existe no codebase atual — apenas `v.x = val`.

## Validação
- `cargo build` — limpo, sem warnings
- `cargo test` — todos os testes que passavam antes continuam passando
- `test_variable` — compila `variables.slynx` que tem `p.age = 55` (FieldAccess assign) e `p = Pessoa(...)` (Identifier assign)
- `test_objects` — progride além do Assign (agora falha em `Component expression is not implemented`, issue pré-existente)

## Limitações
- Nested field access (`a.x.y = 5`) não é suportada — o `get_value_for(parent)` gera `GetField` para o nível mais interno, não o struct intermediário.
- Component expressions (`Counter {}`) continuam `unimplemented!()` em `contexts.rs:149`.
