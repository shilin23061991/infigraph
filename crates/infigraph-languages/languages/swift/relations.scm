; Swift relationship extraction queries

; Function calls
(call_expression
  (simple_identifier) @call.func) @call.site

; Method calls: obj.method()
(call_expression
  (navigation_expression
    (_) @call.receiver
    (navigation_suffix
      (simple_identifier) @call.func))) @call.site

; Import declarations
(import_declaration
  (identifier) @import.module)
