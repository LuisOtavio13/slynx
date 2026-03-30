# Análise Completa do Codebase do Middleend

> Data: 2026-03-29
> Escopo: `middleend/src/`

---

## 1. Visão Geral da Arquitetura

O middleend contém **dois sistemas IR paralelos**:

| Sistema | Local | Estilo | Status |
|---------|-------|--------|--------|
| `SlynxIR` | `ir/` | SSA-style, vetores flat + `IRPointer` bit-packed | **Ativo**, usado em `lib.rs` |
| `IntermediateRepr` | `intermediate/` | Expression-tree, contextos hierárquicos | **Dead code**, não referenciado externamente |

### SlynxIR (ativo)

Estrutura principal em `ir/mod.rs:21-38` — 8 vetores (`Vec`) que armazenam toda a informação:

```
SlynxIR {
    contexts:  Vec<Context>,
    components: Vec<Component>,
    labels:    Vec<Label>,
    instructions: Vec<Instruction>,
    operands:  Vec<Operand>,
    values:    Vec<Value>,
    slots:     Vec<Slot>,
    types:     IRTypes,
    strings:   SymbolsModule,
}
```

O fluxo de geração (`SlynxIR::generate()`) faz:
1. **Hoisting** — cria declarações vazias para objects, functions, components
2. **Preenchimento** — inicializa cada declaração com seus campos reais

### IntermediateRepr (dead code)

Definido em `intermediate/mod.rs`. Declara 5 submodulos que **não existem como arquivos**:

```rust
pub mod context;  // ← não existe
pub mod expr;     // ← existe (intermediate/expr.rs)
pub mod id;       // ← não existe
pub mod node;     // ← não existe
pub mod string;   // ← não existe
pub mod types;    // ← não existe
```

O código compila porque `expr.rs` existe, mas os imports de `context`, `id`, `node`, `string` e `types` no próprio `mod.rs` (linhas 9-16) deveriam causar erro de compilação. Se compila sem erro, o cached incremental do cargo está mascarando o problema — ou o crate não está sendo verificado.

**Recomendação:** Remover completamente `intermediate/` ou mover para um branch de experimentação.

---

## 2. Bugs e Erros de Compilação

### 2.1. Erro de compilação ativo — `ir/contexts.rs:149`

```rust
HirExpressionKind::Component { name, values } => {
    let component = temp.get_component(*name)  // ← falta ;
    unimplemented!("Component expression is not implemented");
}
```

- **Linha 149:** Falta ponto e vírgula. `temp.get_component(*name)` retorna um valor que não é consumido.
- **Erro de tipo:** `name` é `TypeId` (vindo de `HirExpressionKind::Component`), mas `temp.get_component()` provavelmente espera `DeclarationId`. O binding `component` não é usado — o `unimplemented!()` será o único retorno (panico em runtime).

**Correção:** Adicionar `;` e possivelmente trocar `TypeId` por `DeclarationId`, ou implementar a lógica real do componente.

### 2.2. `ir/components.rs` — stub não implementado

`initialize_component` (linhas 6-26) é um stub completo. Todos os match arms são vazios:

```rust
ComponentMemberDeclaration::Property { .. } => {}
ComponentMemberDeclaration::Child { .. } => {}
ComponentMemberDeclaration::Specialized(_) => {}
```

Variáveis `id`, `index`, `span`, `name`, `values` são ignoradas (8 warnings).

### 2.3. `.expect()` com mensagens inconsistentes

Alguns exemplos:

- `intermediate/mod.rs:137`: `"A função deveria ser referenciada, isso tá sendo temporario mas o problema provavel uqe é hoisting"` — mistura português com código, contém typo ("provavel uqe").
- `ir/instructions.rs:64`: `"Variable not found for assignment"` — inglês, ok.
- `ir/instructions.rs:92`: `"Variable not found for field write-back"` — inglês, ok.
- `intermediate/mod.rs:128`: `.unwrap()` sem mensagem em `self.vars.get(&name).unwrap()`.

---

## 3. Code Smells e Problemas Arquiteturais

### 3.1. Duplicação de Código — Helpers de Instrução Binária

9 métodos em `ir/instructions.rs` (linhas 132-238) são **virtualmente idênticos**:

| Método | Linha | Única diferença |
|--------|-------|-----------------|
| `get_add_instruction` | 132 | `Instruction::add()` |
| `get_sub_instruction` | 144 | `Instruction::sub()` |
| `get_mul_instruction` | 156 | `Instruction::mul()` |
| `get_div_instruction` | 168 | `Instruction::div()` |
| `get_cmp_instruction` | 180 | `Instruction::cmp()` |
| `get_gt_instruction` | 192 | `Instruction::gt()` |
| `get_gte_instruction` | 204 | `Instruction::gte()` |
| `get_lt_instruction` | 216 | `Instruction::lt()` |
| `get_lte_instruction` | 228 | `Instruction::lte()` |

Cada método tem corpo idêntico:
```rust
let values = self.insert_value(self.get_value(lhs));
self.insert_value(self.get_value(rhs));
self.insert_instruction(label, Instruction::XXX(ty, values.with_length()))
```

**Solução proposta:** Extrair um método genérico:
```rust
fn get_binary_instruction(
    &mut self,
    lhs: IRPointer<Value, 1>,
    rhs: IRPointer<Value, 1>,
    ty: IRTypeId,
    label: IRPointer<Label, 1>,
    make_instr: fn(IRTypeId, IRPointer<Value, 2>) -> Instruction,
) -> IRPointer<Instruction, 1>
```

### 3.2. Visibilidade Excessiva de Campos

- `IntermediateRepr` em `intermediate/mod.rs:28-38`: **Todos os 8 campos são `pub`**.
- `IRTypeId`, `IRStructId`, `IRFunctionId`, `IRComponentId` em `types/`: todos expõem `pub usize` interno.

Isso quebra encapsulamento e permite mutação arbitrária do estado interno.

### 3.3. Error Handling Inconsistente

Três estilos misturados:

| Estilo | Exemplo | Local |
|--------|---------|-------|
| `Result<T, IRError>` | `get_value_for()` | `ir/contexts.rs` |
| `panic!` / `unreachable!` | `get_slot_for_place()` | `ir/instructions.rs:21-27` |
| `.unwrap()` / `.expect()` | `generate_expr()` | `intermediate/mod.rs:128, 137, 142` |

O enum `IRError` em `error.rs` tem apenas **3 variantes**:

```rust
pub enum IRError {
    IRTypeNotRecognized(TypeId),
    DeclarationNotRecognized(DeclarationId),
    UnrecognizedVariable(VariableId),
}
```

Muitos cenários de erro reais (slot inválido, field out of bounds, componente não encontrado) usam `panic!` em vez de serem representados no enum.

### 3.4. Comentários Doc Duplicados

Em `types/mod.rs`, os 6 métodos getter de tipo usam doc comments **idênticos**:

```rust
///Gets a mutable referente to the type of the function with the provided `id`
pub fn get_function_type(&self, id: IRFunctionId) -> &IRFunction { ... }

///Gets a mutable referente to the type of the function with the provided `id`
pub fn get_object_type(&mut self, id: IRStructId) -> &IRStruct { ... }

///Gets a mutable referente to the type of the function with the provided `id`
pub fn get_component_type(&self, id: IRComponentId) -> &IRComponent { ... }

///Gets a mutable referente to the type of the function with the provided `id`
pub fn get_function_type_mut(&mut self, id: IRFunctionId) -> &mut IRFunction { ... }

///Gets a mutable referente to the type of the function with the provided `id`
pub fn get_object_type_mut(&mut self, id: IRStructId) -> &mut IRStruct { ... }

///Gets a mutable referente to the type of the function with the provided `id`
pub fn get_component_type_mut(&mut self, id: IRComponentId) -> &mut IRComponent { ... }
```

Todos dizem "function" mesmo retornando tipos diferentes. Também há typo: "referente" → "referência".

### 3.5. Mistura de Idiomas nos Comentários

Comentários alternam entre português e inglês sem padrão:

- Português: `"// Letra com acento não pode ser ignorado"` (exemplo típico)
- Inglês: `///Retrieves the raw IR type from the provided id`
- Misturado: `"A função deveria ser referenciada, isso tá sendo temporario mas o problema provavel uqe é hoisting"` (`intermediate/mod.rs:137`)

### 3.6. `create_empty_component` tem bug de índice

`types/mod.rs:140-146`:

```rust
pub fn create_empty_component(&mut self) -> IRTypeId {
    let sout = self.structs.len();        // ← usa structs.len() !
    self.components.push(IRComponent::new());
    let out = self.types.len();
    self.types.push(IRType::Component(IRComponentId(sout)));
    IRTypeId(out)
}
```

O índice `sout` é calculado como `self.structs.len()` mas deveria ser `self.components.len()`. O componente é empilhado em `self.components` mas o `IRComponentId` aponta para a posição errada.

---

## 4. Tipos e Estruturas de Dados

### 4.1. `IRPointer<T, N>` — Bit Packing

Definido em `ir/model/ptr.rs`. Empacota um ponteiro de 48 bits e um length de 16 bits em um `u64`:

- `N = 0`: length é runtime (especificado depois)
- `N > 0`: length é compile-time known

Usado extensivamente para indexar nos vetores flat do `SlynxIR`. Métodos-chave: `ptr()`, `length()`, `with_length()`, `ptr_to_last()`, `null()`.

### 4.2. Hierarquia de Tipos

```
IRType (enum)
├── I8, U8, I16, U16, I32, U32, I64, U64
├── ISIZE, USIZE, F32, F64
├── STR, BOOL, VOID
├── GenericComponent
├── Struct(IRStructId)
├── Function(IRFunctionId)
└── Component(IRComponentId)
```

Builtin types são 16, armazenados em `BUILTIN_TYPES` (`types/mod.rs:10-27`).

### 4.3. Instruções

`InstructionType` enum em `ir/model/instruction.rs` com construtores estáticos:
`add`, `sub`, `mul`, `div`, `cmp`, `gt`, `gte`, `lt`, `lte`, `ret`, `allocate`, `write`, `getfield`, `setfield`, `cbr`, `br`, `raw`.

---

## 5. Módulos e Arquivos

| Arquivo | Linhas | Função | Problemas |
|---------|--------|--------|-----------|
| `lib.rs` | — | Entry point, exports | — |
| `error.rs` | 10 | Enum `IRError` (3 variantes) | Muito limitado |
| `ir/mod.rs` | 118 | `SlynxIR` struct + `generate()` | — |
| `ir/contexts.rs` | 250 | `get_value_for`, `initialize_function` | Erro de compilação linha 149 |
| `ir/instructions.rs` | 392 | 9 helpers duplicados + lógica binária | Duplicação massiva |
| `ir/components.rs` | 27 | `initialize_component` stub | Não implementado (8 warnings) |
| `ir/helper/types.rs` | — | Resolução de tipos | — |
| `ir/helper/contexts.rs` | — | Accessors de contexto | — |
| `ir/temp.rs` | — | `TempIRData` mutable state | — |
| `ir/model/ptr.rs` | — | `IRPointer` bit-packing | — |
| `ir/model/instruction.rs` | — | `InstructionType`, constructors | — |
| `types/mod.rs` | 155 | `IRTypes`, getters, creation | Doc comments duplicados, bug em `create_empty_component` |
| `types/irtype.rs` | — | `IRType` enum, `IRTypeId(pub usize)` | Visibilidade excessiva |
| `types/structs.rs` | — | `IRStruct`, `IRStructId(pub usize)` | Visibilidade excessiva |
| `types/functions.rs` | — | `IRFunction`, `IRFunctionId(pub usize)` | Visibilidade excessiva |
| `types/components.rs` | — | `IRComponent`, `IRComponentId(pub usize)` | Visibilidade excessiva |
| `intermediate/mod.rs` | 420 | `IntermediateRepr` dead code | Submodulos inexistentes |
| `intermediate/expr.rs` | — | `IntermediateExpr` dead code | — |

---

## 6. Resumo de Todos os Problemas Encontrados

### Bugs

| # | Severidade | Local | Descrição |
|---|-----------|-------|-----------|
| B1 | **Crítico** | `ir/contexts.rs:149` | Falta `;` + type mismatch (`TypeId` vs `DeclarationId`) — erro de compilação |
| B2 | **Crítico** | `types/mod.rs:141` | `create_empty_component` usa `structs.len()` em vez de `components.len()` para o índice |
| B3 | **Alto** | `ir/components.rs` | `initialize_component` é stub — componentes não são processados |
| B4 | **Médio** | `intermediate/mod.rs` | Submodulos declarados mas inexistentes — dead code mascarado |

### Code Smells

| # | Tipo | Local | Descrição |
|---|------|-------|-----------|
| S1 | Duplicação | `ir/instructions.rs:132-238` | 9 métodos binários idênticos |
| S2 | Visibilidade | `types/*.rs` | IDs com `pub usize` interno |
| S3 | Visibilidade | `intermediate/mod.rs:28-38` | Todos os campos de `IntermediateRepr` são `pub` |
| S4 | Error Handling | Projeto inteiro | Mistura de `Result`, `panic!`, `.unwrap()` |
| S5 | Documentação | `types/mod.rs:53-90` | 6 doc comments copiados com texto incorreto |
| S6 | Idioma | Projeto inteiro | Comentários em PT/EN misturados |
| S7 | Typos | `intermediate/mod.rs:137` | "provavel uqe" → "provavelmente que" |
| S8 | Dead Code | `intermediate/` | Módulo inteiro não utilizado |

---

## 7. Prioridades de Refatoração

| Prioridade | Item | Impacto | Esforço |
|-----------|------|---------|---------|
| 🔴 P0 | Corrigir erro de compilação em `ir/contexts.rs:149` (B1) | Compilação não passa | Baixo |
| 🔴 P0 | Corrigir bug de índice em `create_empty_component` (B2) | Gera IR corrompido | Baixo |
| 🟡 P1 | Remover módulo `intermediate/` dead code (S8/B4) | Confusão arquitetural | Baixo |
| 🟡 P1 | Implementar `initialize_component` (B3) | Componentes não funcionam | Médio |
| 🟠 P2 | Eliminar duplicação dos 9 helpers binários (S1) | Manutenibilidade | Médio |
| 🟠 P2 | Expandir `IRError` e padronizar error handling (S4) | Robustez | Médio |
| 🟢 P3 | Restringir visibilidade de IDs e campos (S2/S3) | Encapsulamento | Baixo |
| 🟢 P3 | Corrigir doc comments duplicados em `types/mod.rs` (S5) | Documentação | Baixo |
| 🟢 P3 | Padronizar idioma dos comentários (S6/S7) | Consistência | Baixo |

---

## 8. Notas Adicionais

- O projeto parece estar em fase ativa de desenvolvimento, com o `SlynxIR` sendo a implementação real e o `IntermediateRepr` sendo uma versão anterior abandonada.
- O padrão `IRPointer<T, N>` é interessante mas não documentado — uma doc comment explicando a semântica de `N=0` vs `N>0` seria valiosa.
- O módulo `ir/helper/` já separa lógica de acesso, o que é bom. A refatoração dos helpers binários deveria seguir o mesmo padrão.
- O `handle_binary_expression` (instruções.rs:239-366) contém lógica complexa de short-circuit para `&&` e `||` que merece testes unitários dedicados.
