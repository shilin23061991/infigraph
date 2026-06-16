; Scala relationship extraction queries

; Function calls
(call_expression
  (identifier) @call.func) @call.site

; Method calls: obj.method()
(call_expression
  (field_expression
    value: (_) @call.receiver
    field: (identifier) @call.func)) @call.site

; Import declarations
(import_declaration
  (identifier) @import.module)

; Extends clause (inheritance)
(extends_clause
  (type_identifier) @inherit.parent)
