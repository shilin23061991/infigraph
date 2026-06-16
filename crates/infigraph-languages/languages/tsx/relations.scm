; TSX relationship extraction queries (reuses TypeScript/JavaScript grammar)

; Function calls
(call_expression
  function: (identifier) @call.func) @call.site

; Method calls: obj.method()
(call_expression
  function: (member_expression
    object: (_) @call.receiver
    property: (property_identifier) @call.func)) @call.site

; Import statements
(import_statement
  source: (string) @import.module)

; Require calls
(call_expression
  function: (identifier) @_req
  (#eq? @_req "require")
  arguments: (arguments
    (string) @import.module))

; Class inheritance: extends (TSX uses type_identifier for class names)
(class_declaration
  name: (type_identifier) @inherit.child
  (class_heritage
    (extends_clause
      value: (identifier) @inherit.parent)))
