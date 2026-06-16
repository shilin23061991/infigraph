; Go relationship extraction queries

; Function calls
(call_expression
  function: (identifier) @call.func) @call.site

; Method calls: obj.Method()
(call_expression
  function: (selector_expression
    operand: (_) @call.receiver
    field: (field_identifier) @call.func)) @call.site

; Package calls: pkg.Func()
(call_expression
  function: (selector_expression
    operand: (identifier) @_pkg
    field: (field_identifier) @call.func)) @call.site

; Import declarations
(import_spec
  path: (interpreted_string_literal) @import.module)

; Goroutine spawns: go someFunc()
(go_statement
  (call_expression
    function: (identifier) @spawns.target)) @spawns.site

; Goroutine spawns with method: go obj.Method()
(go_statement
  (call_expression
    function: (selector_expression
      field: (field_identifier) @spawns.target))) @spawns.site
